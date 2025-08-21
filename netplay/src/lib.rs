//! This module houses core netplay functionality. It eschews Dolphin's built-in netplay
//! functionality in favor of doing things more low-level.

mod matchmaking;
pub use matchmaking::{MatchmakingManager, MatchmakingState, MatchSearchSettings};

mod netplay;
pub use netplay::{NetplayClient, NetplayConnectionState};

mod pad;

/*#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matchmaking_connect() {
        use slippi_gg_api::APIClient;
        use slippi_shared_types::OnlinePlayMode;
        use slippi_user::UserManager;

        let scm_ver = "knux_test_rs";
        let folder = "/Users/knux/Library/Application Support/com.project-slippi.dolphin/Slippi";
        let api_client = APIClient::new(scm_ver);
        let user_manager = UserManager::new(api_client, folder.into(), scm_ver.into());
        let connect_code = user_manager.get(|user| user.connect_code.clone());

        let mut matchmaking = MatchmakingClient::new(user_manager, scm_ver);

        matchmaking.find_match(MatchSearchSettings {
            mode: OnlinePlayMode::Unranked,
            connect_code
        });

        // Sleep for like 10 secs
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}*/
