use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicI8, Ordering};
use std::thread;

use rusty_enet::{Event, Host, HostSettings, Packet, PacketKind};
use rusty_enet::error::{HostNewError, NoAvailablePeers};
use thiserror::Error;

use dolphin_integrations::Log;
use slippi_shared_types::OnlinePlayMode;
use slippi_user::{UserInfo, UserManager};

const CREATE_TICKET: &str = "create-ticket";
const CREATE_TICKET_RESP: &str = "create-ticket-resp";
const GET_TICKET_RESP: &str = "get-ticket-resp";

use std::borrow::Cow;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct MatchmakingErrorMessage(Arc<OnceLock<Cow<'static, str>>>);

impl MatchmakingErrorMessage {
    pub fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub fn set(&self, value: Cow<'static, str>) {
        if let Err(value) = self.0.set(value) {
            tracing::warn!(target: Log::SlippiOnline, ?value, "MatchmakingErrorMessage double set");
        }
    }

    pub fn get(&self) -> &str {
        match self.0.get() {
            Some(val) => val.as_ref(),
            None => ""
        }
    }
}

/// Represents the current state of the matchmaking service.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchmakingState {
    Idle,
    Initializing,
    Matchmaking,
    OpponentConnecting,
    ConnectionSuccess,
    ErrorEncountered,
}

impl MatchmakingState {
    pub fn to_i8(self) -> i8 {
        match self {
            Self::Idle => 0,
            Self::Initializing => 1,
            Self::Matchmaking => 2,
            Self::OpponentConnecting => 3,
            Self::ConnectionSuccess => 4,
            Self::ErrorEncountered => 5,
        }
    }
}

/// A thread-safe flag that represents the current state of matchmaking.
#[derive(Clone, Debug)]
pub struct MatchmakingStateFlag(Arc<AtomicI8>);

impl MatchmakingStateFlag {
    /// Initializes a new flag.
    pub fn new(state: MatchmakingState) -> Self {
        Self(Arc::new(AtomicI8::new(state.to_i8())))
    }

    pub fn set(&self, state: MatchmakingState) {
        self.0.store(state.to_i8(), Ordering::Release);
    }

    pub fn get(&self) -> MatchmakingState {
        match self.0.load(Ordering::Relaxed) {
            0 => MatchmakingState::Idle,
            1 => MatchmakingState::Initializing,
            2 => MatchmakingState::Matchmaking,
            3 => MatchmakingState::OpponentConnecting,
            4 => MatchmakingState::ConnectionSuccess,
            5 => MatchmakingState::ErrorEncountered,

            // This should never happen, since we don't expose the inner atomic value
            // for setting custom values.
            _ => unreachable!()
        }
    }
}


#[derive(Clone, Debug)]
pub struct MatchSearchSettings {
    pub mode: OnlinePlayMode,
    pub connect_code: String
}

const MM_HOST_DEV: &str = "mm2.slippi.gg";
const MM_HOST_PROD: &str = "mm.slippi.gg";
const MM_PORT: u16 = 43113;

#[derive(Debug)]
pub struct MatchmakingManager {
    pub state: MatchmakingStateFlag,
    pub error_message: MatchmakingErrorMessage,
    pub local_player_index: usize,

    user_manager: UserManager,
    mm_host: &'static str,
    is_mm_connected: bool,
    background_thread: Option<thread::JoinHandle<Option<Host<UdpSocket>>>>,
    player_info: Vec<UserInfo>,
    allowed_stages: Vec<u16>,
}

impl MatchmakingManager {
    /// Creates and returns a new MatchmakingManager instance.
    pub fn new(user_manager: UserManager, scm_ver: &str) -> Self {
        let mm_host = match scm_ver.contains("dev") {
            true => MM_HOST_DEV,
            false => MM_HOST_PROD
        };

        Self {
            user_manager,
            mm_host,
            is_mm_connected: false,
            background_thread: None,
            state: MatchmakingStateFlag::new(MatchmakingState::Idle),
            error_message: MatchmakingErrorMessage::new(),
            local_player_index: 0,
            player_info: Vec::new(),
            allowed_stages: Vec::new(),
        }
    }

    pub fn get_player_info(&self) -> &[UserInfo] {
        &self.player_info
    }

    pub fn remote_player_count(&self) -> usize {
        let count = self.player_info.len();

        if count == 0 {
            return 0;
        }

        count - 1
    }

    pub fn get_player_name(&self, port: usize) -> &str {
        if port >= self.player_info.len() {
            return "";
        }

        &self.player_info[port].display_name
    }

    pub fn get_stages(&self) -> &[u16] {
        &self.allowed_stages
    }

    pub fn get_error_message(&self) -> &str {
        ""
        // &self.error_message
    }

    pub fn is_searching(&self) -> bool {
        let state = self.state.get();
        state == MatchmakingState::Initializing || state == MatchmakingState::Matchmaking
    }

    pub fn find_match(&mut self, settings: MatchSearchSettings) {
        tracing::warn!(target: Log::SlippiOnline, "Starting matchmaking...");
        
        self.is_mm_connected = false;

        // Note that we set a *new* flag here, as we don't want any old threads
        // that haven't exited yet to possibly react to us changing the value
        // behind things.
        self.state = MatchmakingStateFlag::new(MatchmakingState::Initializing);
        self.error_message = MatchmakingErrorMessage::new();

        let user_manager = self.user_manager.clone();
        let mm_host = self.mm_host;
        let state = self.state.clone();
        let error_message = self.error_message.clone();

        let background_thread = thread::spawn(move || {
            run_matchmaking(state, user_manager, mm_host, settings, error_message)
        });

        self.background_thread = Some(background_thread);
    }

    // All the methods we need to scaffold
    // GetNetplayClient (?)
}

/// The core matchmaking operation.
///
/// This should always be called from a background thread.
fn run_matchmaking(
    state: MatchmakingStateFlag,
    user_manager: UserManager,
    mm_host: &'static str,
    _settings: MatchSearchSettings,
    error_message: MatchmakingErrorMessage
) -> Option<Host<UdpSocket>> {
    // We need to set up a few networking related components before we attempt to 
    // do any matchmaking. These are hard requirements for any of the deeper matchmaking
    // states, but it is conceivable that the initial socket resolution and connection to
    // the matchmaking server could see delays. If it has delays, a user could choose to
    // back out of matchmaking; if this happens, we still want to let this thread bail
    // out - hence why `MatchmakingState::Initializing` exists.
    //
    // i.e, if the user hasn't backed out by then, and we're still in the initializing phase,
    // then proceed to matchmaking proper.
    let (mm_socket_addr, mut host, selected_network_port) = match connect_to_mm(mm_host) {
        Ok(values) => values,

        Err(error) => {
            tracing::error!(target: Log::SlippiOnline, ?error, "Failed matchmaking network setup");

            error_message.set(match error {
                ConnectError::ServerLookup(_) => "Failed to find mm server".into(),
                ConnectError::NoValidServerAddr => "Failed to route to mm server".into(),
                
                ConnectError::HostNew(_) |
                ConnectError::SocketBind(_) |
                ConnectError::SocketPortCheck(_) => "Failed to create mm client".into(),

                ConnectError::NoAvailablePeers(_) |
                ConnectError::HostRead(_) |
                ConnectError::UnableToConnect => "Failed to start connection to mm server".into()
            });

            state.set(MatchmakingState::ErrorEncountered);
            return None;
        }
    };

    let lan_addr = match determine_lan_addr(mm_socket_addr, selected_network_port) {
        Ok(lan_addr) => lan_addr,

        Err(error) => {
            tracing::error!(target: Log::SlippiOnline, ?error, "Failed matchmaking network setup");
            error_message.set("Unable to determine IP addr".into());
            state.set(MatchmakingState::ErrorEncountered);
            return None;
        }
    };

    // This loop, at a glance, seems like it could be done away with - but it's
    // important to understand that the matchmaking `state` acts as a checkpoint
    // that the main/game thread can use to interrupt the flow - e.g, if a user 
    // starts a search then cancels it.
    //
    // If this had no interrupt points, the thread - even if detached - would continue
    // along, business as usual, and wind up "ghost connecting" to another player.
    loop {
        match state.get() {
            MatchmakingState::Initializing => {
                let Err(error) = submit_ticket(&mut host, &user_manager, &lan_addr) else {
                    state.set(MatchmakingState::Matchmaking);
                    continue;
                };
                
                tracing::error!(target: Log::SlippiOnline, ?error, "Matchmaking init failure");

                error_message.set(match error {
                    SubmitTicketError::InvalidBody(_) => "Failed to submit to mm queue".into(),
                    SubmitTicketError::Receive(_) => "Failed to join mm queue".into(),
                    SubmitTicketError::InvalidResponse(_) => "Invalid response from mm queue".into(),
                    SubmitTicketError::Server(error) => error.into()
                });

                state.set(MatchmakingState::ErrorEncountered);
                return None;
            },

            MatchmakingState::Matchmaking => {
                let Err(error) = handle_matchmaking(&mut host) else {
                    state.set(MatchmakingState::OpponentConnecting);
                    continue;
                };

                // A timeout here simply means we don't have a match given to us yet, so we can
                // continue waiting around until one is provided - or the connection breaks, or
                // the user backs out.
                if let MatchmakeError::Receive(ReceiveError::Timeout) = &error {
                    tracing::info!(target: Log::SlippiOnline, "Have not yet received assignment");
                    continue;
                }
                
                tracing::error!(target: Log::SlippiOnline, ?error, "Matchmaking failure");

                error_message.set(match error {
                    MatchmakeError::Receive(ReceiveError::Disconnect) => "Lost connection to the mm server".into(),
                    MatchmakeError::Receive(_) => "Failed to receive mm status".into(),
                    MatchmakeError::InvalidResponse(_) => "Invalid response when getting mm status".into(),
                    MatchmakeError::Server(error) => error.into()
                });

                state.set(MatchmakingState::ErrorEncountered);
                return None;
            },

            MatchmakingState::OpponentConnecting => {
                // check ticket status and react~, etc
            },

            // Once we hit any other state, by any other means, this thread
            // should exit. The user will kick off another run if they begin
            // searching for a new match.
            _ => { break; }
        }
    }

    Some(host)
}

impl Drop for MatchmakingManager {
    fn drop(&mut self) {
        self.state.set(MatchmakingState::Idle);

        if let Some(background_thread) = self.background_thread.take() {
            if let Err(error) = background_thread.join() {
                tracing::error!(target: Log::SlippiOnline, ?error, "Unable to join background thread");
            }
        }
    }
}

#[derive(Debug, Error)]
enum ReceiveError {
    #[error(transparent)]
    HostRead(std::io::Error),

    #[error(transparent)]
    Deserialize(serde_json::Error),

    #[error("Matchmaking server disconnected")]
    Disconnect,

    #[error("No response from matchmaking server")]
    Timeout,

    #[error(transparent)]
    Utf8Read(std::str::Utf8Error)
}

/// Repeatedly checks the inner socket for new data. We will attempt to deserialize any data
/// received to our expected type.
///
/// This attempts to replicate the timeout handling of the C++ version, albeit against what
/// appears to be a newer/different enet API. For the way this is called, it's not a
/// significant burden to just chunk the timeout checking manually 
/// (e.g 5000ms in 250ms chunks, etc).
fn receive<T>(host: &mut Host<UdpSocket>, mut timeout_ms: i32) -> Result<T, ReceiveError>
where
    T: serde::de::DeserializeOwned,
{
    let host_service_timeout_ms = 250;

    // Make sure loop runs at least once
    if timeout_ms < host_service_timeout_ms {
        timeout_ms = host_service_timeout_ms;
    }

    // This is not a perfect way to timeout but hopefully it's close enough?
    let max_attempts = timeout_ms / host_service_timeout_ms;

    tracing::info!(target: Log::SlippiOnline, ?max_attempts, "Waiting for enet event");
    
    let mut attempt = 0;

    while attempt < max_attempts {
        tracing::info!(target: Log::SlippiOnline, ?attempt, "Checking for enet event");

        if let Some(event) = host.service().map_err(ReceiveError::HostRead)? {
            if let Event::Disconnect { .. } = event {
                return Err(ReceiveError::Disconnect);
            }

            if let Event::Receive { peer: _, channel_id: _, packet } = event {
                let message = str::from_utf8(packet.data()).map_err(ReceiveError::Utf8Read)?;
                let data = serde_json::from_str(message).map_err(ReceiveError::Deserialize)?;
                return Ok(data);
            }
        }

        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    Err(ReceiveError::Timeout)
}

#[derive(Debug, Error)]
enum ConnectError {
    #[error(transparent)]
    ServerLookup(std::io::Error),

    #[error("Failed to determine socket addrs for matchmaking server url")]
    NoValidServerAddr,

    #[error(transparent)]
    SocketBind(std::io::Error),

    #[error(transparent)]
    SocketPortCheck(std::io::Error),

    #[error(transparent)]
    HostNew(HostNewError<UdpSocket>),

    #[error(transparent)]
    NoAvailablePeers(NoAvailablePeers),

    #[error(transparent)]
    HostRead(std::io::Error),

    #[error("Failed to connect to matchmaking server")]
    UnableToConnect
}

/// Creates a new enet host client, connected to the matchmaking server and ready for
/// further usage.
fn connect_to_mm(mm_host: &str) -> Result<(SocketAddr, Host<UdpSocket>, u16), ConnectError> {
    // There's no sense in doing anything further if we can't resolve the socket addr 
    // for the matchmaking server.
    let mm_socket_addr = (mm_host, MM_PORT)
        .to_socket_addrs()
        .map_err(ConnectError::ServerLookup)?
        .next()
        .ok_or_else(|| ConnectError::NoValidServerAddr)?;

    let addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

    let get_dolphin_custom_netplay_port: fn() -> Option<u16> = || { None };

    // Generate a list of addresses & ports to try. 
    // 
    // We are explicitly trying a slew of client addresses because we are trying to utilize
    // our connection to the matchmaking service in order to hole punch. Whichever port works
    // will end up being the port we listen on when we start our server.
    let port_attempts = match get_dolphin_custom_netplay_port() {
        Some(port) => vec![SocketAddr::new(addr, port)],
        
        None => (0..15).into_iter().map(|i| {
            // 41000 + (generator() % 10000);
            SocketAddr::new(addr, 41000 + i)
        }).collect()
    };
    
    tracing::warn!(target: Log::SlippiOnline, ?port_attempts);

    let socket = UdpSocket::bind(&*port_attempts).map_err(ConnectError::SocketBind)?;
    let port = socket.local_addr().map_err(ConnectError::SocketPortCheck)?.port();

    let mut host = Host::new(socket, HostSettings {
        peer_limit: 1,
        channel_limit: 3,
        incoming_bandwidth_limit: None,
        outgoing_bandwidth_limit: None,
        ..Default::default()
    }).map_err(ConnectError::HostNew)?;

    host.connect(mm_socket_addr, 3, 0).map_err(ConnectError::NoAvailablePeers)?;

    // Listen to the host for a short period so we can make sure we're properly connected.
    let mut attempt = 0;
    let max_attempts = 20;
    loop {
        match host.service().map_err(ConnectError::HostRead)? {
            Some(Event::Connect { .. }) => {
                // is_mm_connected = true;
                return Ok((mm_socket_addr, host, port));
            },

            Some(event) => {
                tracing::warn!(
                    target: Log::SlippiOnline,
                    ?event,
                    "Received unexpected event in client initialization"
                );
            },

            None => {}
        }

        if attempt == max_attempts {
            // set state = error
            // set error message
            return Err(ConnectError::UnableToConnect);
        }

        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Determine local IP address. We can attempt to connect to our opponent via
/// local IP address if we have the same external IP address. The following
/// scenarios can cause us to have the same external IP address:
///
/// - we are connected to the same LAN
/// - we are connected to the same VPN node
/// - we are behind the same CGNAT
fn determine_lan_addr(mm_addr: SocketAddr, port: u16) -> Result<String, std::io::Error> {
    let get_dolphin_custom_lan_ip: fn() -> Option<String> = || { None };

    match get_dolphin_custom_lan_ip() {
        Some(addr) => {
            tracing::warn!(target: Log::SlippiOnline, "Overwriting LAN IP with custom address");
            Ok(format!("{addr}:{port}"))
        },

        None => {
            let socket = UdpSocket::bind("0.0.0.0:0")?;
            socket.connect(mm_addr)?;

            let local_addr = socket.local_addr()?.ip();
            Ok(format!("{local_addr}:{port}"))
        }
    }
}

/// Any errors that can occur during the ticket submission process.
#[derive(Debug, Error)]
enum SubmitTicketError {
    #[error(transparent)]
    InvalidBody(serde_json::Error),

    #[error(transparent)]
    Receive(ReceiveError),

    #[error("Invalid response type from server: {0}")]
    InvalidResponse(String),

    #[error("Error from server: {0}")]
    Server(String)
}

/// The response payload format we expect from successful ticket submission.
#[derive(Debug, serde::Deserialize)]
struct SubmitTicketResponse {
    #[serde(alias = "type")]
    kind: String,
    error: Option<String>
}

/// Submits a match ticket to the matchmaking server.
fn submit_ticket(
    host: &mut Host<UdpSocket>,
    user_manager: &UserManager,
    lan_addr: &str
) -> Result<(), SubmitTicketError> {
    let (uid, play_key, connect_code, display_name) = user_manager.get(|user| {
        (user.uid.clone(), user.play_key.clone(), user.connect_code.clone(), user.display_name.clone())
    });

    let request = serde_json::json!({
        "type": CREATE_TICKET,
        "user": {
            "uid": uid,
            "playKey": play_key,
            "connectCode": connect_code,
            "displayName": display_name
        },
        "search": {
            "mode": 1, // Unranked
            "connectCode": ""
        },
        "appVersion": "3.5.1",
        "ipAddressLan": lan_addr
    });

    let request_body = serde_json::to_string(&request)
        .map_err(SubmitTicketError::InvalidBody)?;

    tracing::info!(target: Log::SlippiOnline, ticket_request = ?request_body);

    let packet = Packet::new(request_body.as_bytes(), PacketKind::Reliable);
    let channel_id = 0;
    host.broadcast(channel_id, &packet);

    let response: SubmitTicketResponse = receive(host, 5000)
        .map_err(SubmitTicketError::Receive)?;

    tracing::info!(target: Log::SlippiOnline, ticket_response = ?response);

    if response.kind != CREATE_TICKET_RESP {
        return Err(SubmitTicketError::InvalidResponse(response.kind));
    }

    if let Some(error) = response.error {
        return Err(SubmitTicketError::Server(error));
    }

    Ok(())
}

#[derive(Debug, Error)]
enum MatchmakeError {
    #[error(transparent)]
    Receive(ReceiveError),

    #[error("Invalid response type from server: {0}")]
    InvalidResponse(String),

    #[error("Error from server: {0}")]
    Server(String)
}

#[derive(Debug, serde::Deserialize)]
struct TicketResponse {
    #[serde(alias = "type")]
    pub kind: String,
    
    #[serde(alias = "latestVersion")]
    pub latest_version: Option<String>,

    #[serde(alias = "matchId")]
    pub match_id: Option<String>,

    pub error: Option<String>,
    pub players: Option<Vec<serde_json::Value>>,
    pub stages: Option<Vec<serde_json::Value>>,

    #[serde(alias = "isHost")]
    pub is_host: bool
}

fn handle_matchmaking(
    host: &mut Host<UdpSocket>
) -> Result<(), MatchmakeError> {
    let response: TicketResponse = receive(host, 2000).map_err(MatchmakeError::Receive)?;
    
    tracing::info!(target: Log::SlippiOnline, mm_response = ?response);

    if response.kind != GET_TICKET_RESP {
        return Err(MatchmakeError::InvalidResponse(response.kind));
    }

    if let Some(error) = response.error {
        // @TODO: Overwrite from MM
        return Err(MatchmakeError::Server(error));
    }

    Ok(())
}
