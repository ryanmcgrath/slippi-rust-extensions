use std::thread;

use slippi_shared_types::OnlinePlayMode;
use slippi_user::{UserInfo, UserManager};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchmakingState {
    Idle,
    Initializing,
    Matchmaking,
    OpponentConnecting,
    ConnectionSuccess,
    ErrorEncountered
}

#[derive(Clone, Debug)]
pub struct MatchSearchSettings {
    mode: OnlinePlayMode,
    connect_code: String
}

const MM_HOST_DEV: &str = "mm2.slippi.gg";
const MM_HOST_PROD: &str = "mm.slippi.gg";
const MM_PORT: u16 = 43113;

#[derive(Debug)]
pub struct MatchmakingClient {
    user_manager: UserManager,
    mm_host: &'static str,
    is_mm_connected: bool,
    client: Option<i32>, // enet
    server: Option<i32>, // enet
    // Netplay client
    // random_number_generator
    background_thread: Option<thread::JoinHandle<()>>,
    search_settings: Option<MatchSearchSettings>,
    pub state: MatchmakingState,
    pub error_message: String,
    is_swap_attempt: bool,
    host_port: Option<i32>,
    pub local_player_index: usize,
    remote_ips: Vec<String>,
    player_info: Vec<UserInfo>,
    allowed_stages: Vec<u16>,
    joined_lobby: bool,
    is_host: bool
}

impl MatchmakingClient {
    /// Creates and returns a new MatchmakingClient instance.
    pub fn new(
        user_manager: UserManager,
        scm_ver: &str
    ) -> Self {
        let mm_host = match scm_ver.contains("dev") {
            true => MM_HOST_DEV,
            false => MM_HOST_PROD
        };

        Self {
            user_manager,
            mm_host,
            client: None,
            server: None,
            is_mm_connected: false,
            background_thread: None,
            search_settings: None,
            state: MatchmakingState::Idle,
            error_message: String::new(),
            is_swap_attempt: false,
            host_port: None,
            local_player_index: 0,
            remote_ips: Vec::new(),
            player_info: Vec::new(),
            allowed_stages: Vec::new(),
            joined_lobby: false,
            is_host: false
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
        &self.error_message
    }

    pub fn find_match(&self) {
        // Kick off background thread, etc
    }

    pub fn reset(&self) {

    }

    // All the methods we need to scaffold
    // GetNetplayClient (?)
}

impl Drop for MatchmakingClient {
    fn drop(&mut self) {
        if let Some(background_thread) = self.background_thread.take() {
            if let Err(error) = background_thread.join() {
                eprintln!("...");
            }
        }
    }
}
