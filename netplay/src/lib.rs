//! This module houses core netplay functionality. It eschews Dolphin's built-in netplay
//! functionality in favor of doing things more low-level.

mod matchmaking;
pub use matchmaking::{MatchmakingClient, MatchmakingState};

mod netplay;
pub use netplay::{NetplayClient, NetplayConnectionState};

mod pad;
mod utils;
