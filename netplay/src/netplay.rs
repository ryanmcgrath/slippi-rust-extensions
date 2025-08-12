use std::thread;

use crate::utils::{Flag, Queue};

// Number of frames to wait before attempting to time-sync
const ONLINE_LOCKSTEP_INTERVAL: usize = 30;

const REMOTE_PLAYER_MAX: usize = 3;
const PING_DISPLAY_INTERVAL: usize = 60;
const REMOTE_PLAYER_COUNT: usize = 3;

#[derive(Debug)]
pub struct FrameTiming {
    frame: i32,
    time_us: u64
}

#[derive(Debug)]
pub struct FrameOffsetData {
    // TODO: Should the buffer size be dynamic based on time sync interval or not?
    index: i32,
    buffer: Vec<i32>
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConnectionStatus {
    Unset,
    Initiated,
    Connected,
    Failed,
    Disconnected
}

#[derive(Clone, Debug)]
struct RemotePadOutput {
    latest_frame: i32,
    player_index: u8,
    data: Vec<u8>
}

#[derive(Debug)]
struct Packet;

#[derive(Clone, Copy, Debug)]
struct PlayerSelections {
    player_index: u8,
    character_id: u8,
    character_color: u8,
    team_id: u8,
    is_character_selected: bool,
    stage_id: u16,
    is_stage_selected: bool,
    rng_offset: u32,
    message_id: i32,
    error: bool
}

impl Default for PlayerSelections {
    fn default() -> Self {
        Self {
            player_index: 0,
            character_id: 0,
            character_color: 0,
            team_id: 0,
            is_character_selected: false,
            stage_id: 0,
            is_stage_selected: false,
            rng_offset: 0,
            message_id: 0,
            error: false
        }
    }
}

impl PlayerSelections {
    pub fn merge(&mut self, sel: &Self) {
        self.rng_offset = sel.rng_offset;

        if sel.is_stage_selected {
            self.stage_id = sel.stage_id;
            self.is_stage_selected = true;
        }

        if sel.is_character_selected {
            self.character_id = sel.character_id;
            self.character_color = sel.character_color;
            self.team_id = sel.team_id;
            self.is_character_selected = true;
        }
    }

    pub fn reset(&mut self) {
        self.character_id = 0;
        self.character_color = 0;
        self.is_character_selected = false;
        self.team_id = 0;
        self.stage_id = 0;
        self.is_stage_selected = false;
        self.rng_offset = 0;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchInfo {
    local: PlayerSelections,
    remote: [PlayerSelections; REMOTE_PLAYER_MAX]
}

impl MatchInfo {
    pub fn reset_selections(&mut self) {
        self.local.reset();

        for entry in self.remote.iter_mut() {
            entry.reset();
        }
    }
}

/// Represents the current connection state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetplayConnectionState {
    Unset,
    Initiated,
    Connected,
    Failed,
    Disconnected
}

#[derive(Debug)]
pub struct RemotePlayerAddress {
    addr: String,
    port: u16
}

#[derive(Debug)]
pub struct NetplayClient {
    local_player_port: u8,
    remote_player_count: u8,
    match_info: MatchInfo,
    do_loop: Flag,
    is_connection_selected: bool,
    // has_game_started: bool,
    pub is_decider: bool,
    queue: Queue<Packet>,
    thread: Option<thread::JoinHandle<()>>,
}

impl NetplayClient {
    /// Creates and returns a new `NetplayClient`.
    ///
    /// It is important to understand that the client is not usable yet! You
    /// are required to call `.initialize()` with appropriate parameters from a
    /// matchmaking client to properly configure this.
    ///
    /// (We have this `new()` method to avoid dealing with a rather confusing
    /// `Option<NetplayClient>` type on the EXI device, which gets even more
    /// confusing to follow when we're currently bridging over FFI with Dolphin.
    /// This distinction can be revised in the future as more things move in to
    /// Rust.)
    pub fn new() -> Self {
        Self {
            local_player_port: 0,
            remote_player_count: 0,
            match_info: MatchInfo::default(),
            do_loop: Flag::new(false),
            is_connection_selected: false,
            is_decider: false,
            queue: Queue::new(),
            thread: None,
        }
    }

    pub fn initialize(
        &mut self,
        remote_players: Vec<RemotePlayerAddress>,
        local_address_port: u16,
        local_player_port: u8,
        is_decider: bool
    ) {
        self.local_player_port = local_player_port;
        self.is_decider = is_decider;
        self.match_info = MatchInfo::default();
        self.remote_player_count = remote_players.len() as u8;

        let (mut i, mut j) = (0, 0);

        while i < REMOTE_PLAYER_MAX {
            if j == local_player_port {
                j += 1;
            }

            self.match_info.remote[i].player_index = j;

            // other stuff...

            i += 1;
            j += 1;
        }

        self.thread = Some(thread::spawn(|| {
            loop {}
        }));
    }

    pub fn start_game(&self) {}
    pub fn reset(&mut self) {}
    pub fn drop_old_remote_inputs(&self) {}

    pub fn get_connection_state(&self) -> NetplayConnectionState {
        NetplayConnectionState::Unset
    }

    pub fn player_index_from_port(&self, mut port: u8) -> u8 {
        if port > self.local_player_port {
            port -= 1;
        }

        port
    }

    pub fn get_remote_pad(&self, current_frame: i32, index: i32) -> RemotePadOutput {
        unimplemented!()
    }

    pub fn send_async(&self, packet: Packet) {

    }
}

impl Drop for NetplayClient {
    fn drop(&mut self) {
        self.do_loop.set(false);

        // Cleanup any ENet stuff, etc.
    }
}
