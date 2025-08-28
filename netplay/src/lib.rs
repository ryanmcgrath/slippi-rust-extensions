//! This module houses core netplay functionality. It eschews Dolphin's built-in netplay
//! functionality in favor of doing things more low-level.

use std::borrow::Cow;
use std::thread;

use dolphin_integrations::Log;
use slippi_gg_api::APIClient;
use slippi_shared_types::{AtomicState, OnceValue};
use slippi_user::UserManager;

mod enet;

mod context;
use context::MatchContext;
pub use context::Stage;

mod matchmaking;
pub use matchmaking::MatchSearchSettings;

mod netplay;
pub use netplay::{NetplayClient, NetplayConnectionState};

mod pad;

mod state;
pub use state::NetplayState;

/// Entry point for managing netplay sessions.
///
/// Note that this is always a snapshot of the current netplay interaction. When we move to a
/// new session (e.g, by initiating a search for a new match), we detach all state and let the
/// background thread(s) drop the data when they're done. New threads that are created get their
/// own new state to work with.
///
/// The general flow of this can be thought of as the following:
///
/// ```
/// -------------------     ----------------------------     ------------------------     
/// | find_match(...) | --> | Matchmaking thread spawn | --> | Netplay thread spawn |
/// -------------------     ----------------------------     ------------------------
/// ```
///
/// See the documentation of `find_match` for more information.
#[derive(Debug)]
pub struct NetplayManager {
    pub state: AtomicState<NetplayState>,
    pub context: OnceValue<MatchContext>,
    pub error: OnceValue<Cow<'static, str>>,

    api_client: APIClient,
    user_manager: UserManager,
    scm_ver: String
}

impl NetplayManager {
    /// Initializes a new `NetplayManager`.
    pub fn new(api_client: APIClient, user_manager: UserManager, scm_ver: String) -> Self {
        Self {
            state: AtomicState::new(NetplayState::Idle),
            context: OnceValue::new(),
            error: OnceValue::new(),
            api_client,
            user_manager,
            scm_ver
        }
    }

    /// Returns the current error message; for reasons currently related to FFI and
    /// not wanting to deal with `None`, this is effectively always a blank string
    /// unless there's an actual value held.
    pub fn get_error_message(&self) -> &str {
        match self.error.get() {
            Some(val) => val.as_ref(),
            None => ""
        }
    }

    pub fn remote_player_count(&self) -> usize {
        match self.context.get() {
            Some(context) => context.players.len() - 1,
            None => 0
        }
    }

    pub fn get_stages(&self) -> &[Stage] {
        match self.context.get() {
            Some(context) => &context.stages,
            None => &[]
        }
    }

    pub fn get_player_name(&self, port: usize) -> &str {
        match self.context.get() {
            Some(context) => match context.players.get(port) {
                Some(player) => &player.display_name,
                None => ""
            },

            None => ""
        }
    }

    /// Returns whether we're in matchmaking search mode.
    pub fn is_searching(&self) -> bool {
        let state = self.state.get();
        state == NetplayState::Initializing || state == NetplayState::Matchmaking
    }

    /// Kicks off a search for a new match.
    ///
    /// Under the hood, this method kicks off a new background thread that handles the flow
    /// of searching for a new match. If that background thread is successful, it then spawns
    /// another background thread that handles the netplay network connection. There's a few
    /// advantages to this architecture:
    ///
    /// First, we create a lock-free channel before spawning the matchmaking thread. The matchmaking
    /// thread ensures that the receiver side is passed to the netplay thread if we wind up with a
    /// match to play. This ensures that we can communicate with any eventual netplay client thread
    /// without needing to do a ton of housekeeping in this module.
    ///
    /// Second, this allows the matchmaking thread to do any housekeeping it needs when it's done.
    /// Things like enet deinitialization (etc) can take time and need to happen on a background
    /// thread, but since they're already over there anyway we can just spawn the netplay thread
    /// from there and let matchmaking wither away.
    pub fn find_match(&mut self, settings: MatchSearchSettings) {
        tracing::info!(target: Log::SlippiOnline, "Starting matchmaking...");

        // Set any existing state to `Idle` in case we're replacing an existing operation.
        // This will cause any background thread to finish and exit, disposing of resources
        // asynchronously.
        self.state.set(NetplayState::Idle);

        // Make sure we initialize new flags to match the current background thread
        // state - i.e, the new thread should not be able to change old values.
        self.state = AtomicState::new(NetplayState::Initializing);
        self.context = OnceValue::new();
        self.error = OnceValue::new();

        let api_client = self.api_client.clone();
        let user_manager = self.user_manager.clone();
        let state = self.state.clone();
        let context = self.context.clone();
        let error = self.error.clone();
        let scm_ver = self.scm_ver.clone();

        let result = thread::Builder::new()
            .name("SlippiMatchmakingThread".into())
            .spawn(move || {
                matchmaking::run(state, context, error, scm_ver, api_client, user_manager, settings);
            });

        // It is unlikely this would ever be an issue.
        if let Err(error) = result {
            tracing::error!(target: Log::SlippiOnline, ?error, "Failed to launch matchmaking thread");
            self.error.set("Failed to start mm".into());
            self.state.set(NetplayState::ErrorEncountered);
        }
    }
}
