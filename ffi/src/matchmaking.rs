use std::ffi::{CString, c_char, c_int, c_ushort};

use slippi_exi_device::SlippiEXIDevice;
use slippi_netplay::MatchmakingState;
use slippi_shared_types::OnlinePlayMode;
use slippi_user::UserInfo;

use crate::{with, with_returning};
use crate::game_reporter::SlippiMatchmakingOnlinePlayMode;
use crate::user::RustUserInfo;

/// Returns the index of the local player.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_local_player_idx(exi_device_instance_ptr: usize) -> c_int {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        device.matchmaking.local_player_index as c_int
    })
}

/// Initiates a search for a new match.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_find_match(exi_device_instance_ptr: usize) {
    with::<SlippiEXIDevice, _>(exi_device_instance_ptr, |_device| {
        // device.matchmaking.find_match();
    })
}

/// Returns whatever current error string is on the matchmaking client. This value
/// needs to be free'd from the Rust side and callers should make sure they do so via
/// the provided generic method at the root of this crate.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_error_message(exi_device_instance_ptr: usize) -> *const c_char {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        let msg = device.matchmaking.error_message.get();
        CString::new(msg).expect("slprs_mm_get_error_message failed").into_raw()
    })
}

/// Returns the total number of known remote players.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_remote_player_count(exi_device_instance_ptr: usize) -> c_int {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        device.matchmaking.remote_player_count() as c_int
    })
}

/// Checks whether we're in a fixed-rules-mode or not.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_is_fixed_rules_mode(mode: SlippiMatchmakingOnlinePlayMode) -> bool {
    match mode {
        SlippiMatchmakingOnlinePlayMode::Ranked => OnlinePlayMode::Ranked,
        SlippiMatchmakingOnlinePlayMode::Unranked => OnlinePlayMode::Unranked,
        SlippiMatchmakingOnlinePlayMode::Direct => OnlinePlayMode::Direct,
        SlippiMatchmakingOnlinePlayMode::Teams => OnlinePlayMode::Teams
    }.is_fixed_rules_mode()
}

/// Gets the name for the player in the specific port.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_player_name(
    exi_device_instance_ptr: usize,
    port: c_int
) -> *const c_char {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        let name = device.matchmaking.get_player_name(port as usize);
        CString::new(name).expect("slprs_mm_get_player_name failed").into_raw()
    })
}

/// A C-compatible version of the MatchmakingState enum that can be referenced on the
/// Dolphin side. This will go away once we're mostly in Rust.
#[repr(C)]
pub enum SlippiMatchmakingState {
    Idle = 0,
    Initializing = 1,
    Matchmaking = 2,
    OpponentConnecting = 3,
    ConnectionSuccess = 4,
    ErrorEncountered = 5
}

/// Returns the current state of the matchmaking process.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_matchmake_state(
    exi_device_instance_ptr: usize,
) -> SlippiMatchmakingState {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        match device.matchmaking.state.get() {
            MatchmakingState::Idle => SlippiMatchmakingState::Idle,
            MatchmakingState::Initializing => SlippiMatchmakingState::Initializing,
            MatchmakingState::Matchmaking => SlippiMatchmakingState::Matchmaking,
            MatchmakingState::OpponentConnecting => SlippiMatchmakingState::OpponentConnecting,
            MatchmakingState::ConnectionSuccess => SlippiMatchmakingState::ConnectionSuccess,
            MatchmakingState::ErrorEncountered => SlippiMatchmakingState::ErrorEncountered
        }
    })
}

/// An intermediary type for moving chat messages across the FFI boundary.
///
/// This type is C compatible, and we coerce Rust types into C types for this struct to
/// ease passing things over. This must be free'd on the Rust side via `slprs_mm_free_stages`.
#[repr(C)]
pub struct RustStageList {
    pub data: *mut *mut c_ushort,
    pub len: c_int,
}

impl RustStageList {
    pub fn from(stages: &[u16]) -> Self {
        let mut stages: Vec<*mut _> = stages
            .iter()
            .map(|val| *val as *mut c_ushort)
            .collect();

        stages.shrink_to_fit();

        let len = stages.len() as c_int;
        let data = stages.as_mut_ptr();
        std::mem::forget(stages);

        Self { data, len }
    }
}

/// Returns the current stage list the matchmaking service is working with.
///
/// The returned type must be freed with the corresponding method, as the Rust allocator
/// is different than the C/C++ ones.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_stages(exi_device_instance_ptr: usize) -> RustStageList {
    // To move an array back, we'll create a Vec, shrink it, and stash the len and pointer
    // on the struct we're returning. The C++ side can unravel as necessary, and the free
    // method in this module should handle cleaning this up.
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        RustStageList::from(device.matchmaking.get_stages())
    })
}

/// Takes ownership back of a `RustStageList` struct and drops it.
///
/// When the C/C++ side grabs `RustStage`, it needs to ensure that it's passed back to Rust
/// to ensure that the memory layout matches - do _not_ call `free` on `RustStageList`, pass it
/// here instead.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_free_stages(ptr: *mut RustStageList) {
    if ptr.is_null() {
        // Log here~?
        return;
    }

    // Rebuild the Vec~
    // It'll drop shortly, and they're just u16's inside so it 
    // should be simple now - it's the Vec allocation we need to clean.
    unsafe {
        let stages = Box::from_raw(ptr);
        let len = stages.len as usize;
        let _stages = Vec::from_raw_parts(stages.data, len, len);
    }
}

/// Instructs the matchmaking service to reset internal state.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_reset(exi_device_instance_ptr: usize) {
    with::<SlippiEXIDevice, _>(exi_device_instance_ptr, |device| {
        // device.matchmaking.reset();
    })
}

/// An intermediary type for moving a list of user info across the FFI boundary.
///
/// This type is C compatible, and we coerce Rust types into C types for this struct to
/// ease passing things over. This must be free'd on the Rust side via `slprs_mm_free_stages`.
#[repr(C)]
pub struct RustUserList {
    pub data: *mut *mut RustUserInfo,
    pub len: c_int,
}

impl RustUserList {
    pub fn from(users: &[UserInfo]) -> Self {
        let mut users: Vec<_> = users
            .iter()
            .map(|user| {
                let user = Box::new(RustUserInfo::from(user));
                Box::into_raw(user)
            })
            .collect();

        users.shrink_to_fit();

        let len = users.len() as c_int;
        let data = users.as_mut_ptr();
        std::mem::forget(users);

        Self { data, len }
    }
}

/// Get information for all the current players in the matchmaking service.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_player_info(exi_device_instance_ptr: usize) -> RustUserList {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        RustUserList::from(device.matchmaking.get_player_info())
    })
}

/// C-Compatible struct for returning match result data.
#[repr(C)]
pub struct MatchmakeResult {
    pub id: *const c_char,
    pub players: RustUserList,
    pub stages: RustStageList
}

#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_matchmake_result(exi_device_instance_ptr: usize) -> MatchmakeResult {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |_device| {
        MatchmakeResult {
            id: CString::new("").expect("").into_raw(),
            players: RustUserList::from(&[]),
            stages: RustStageList::from(&[])
        }
    })
}

/// C-compatible enum for representing rank. This will disappear as things
/// move further in to Rust.
#[repr(C)]
pub enum RustSlippiRank {
    Unranked
}

/// Returns the current rank for the player.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_mm_get_player_rank(exi_device_instance_ptr: usize) -> RustSlippiRank {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |_device| {
        RustSlippiRank::Unranked
    })
}

// GetNetplayClient()
// handle connection cleanup (replace client on EXI Device? May not be necessary)
