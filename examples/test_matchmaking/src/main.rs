use tracing_subscriber::prelude::*;

use slippi_gg_api::APIClient;
use slippi_netplay::{NetplayManager, MatchSearchSettings, NetplayState};
use slippi_shared_types::OnlinePlayMode;
use slippi_user::UserManager;

fn main() {
    unsafe { std::env::set_var("RUST_LOG", "info,ureq=warn"); }

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().compact())
        .init();

    let scm_ver = "knux_test_rs";
    let folder = "/Users/knux/Library/Application Support/com.project-slippi.dolphin/Slippi";
    let api_client = APIClient::new(scm_ver);
    
    let user_manager = UserManager::new(api_client, folder.into(), scm_ver.into());
    user_manager.attempt_login();

    let mut matchmaking = NetplayManager::new();

    matchmaking.find_match("3.5.1", user_manager, MatchSearchSettings {
        mode: OnlinePlayMode::Unranked,
        connect_code: String::new()
    });

    loop {
        if matchmaking.state.get() == NetplayState::OpponentConnecting {
            tracing::info!("Found opponent, stopping!");
            std::process::exit(1);
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
