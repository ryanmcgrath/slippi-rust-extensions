use std::ffi::{CString, c_char, c_uchar, c_int, c_uint, c_ushort};

use slippi_exi_device::SlippiEXIDevice;
use slippi_netplay::NetplayConnectionState;
use slippi_shared_types::OnlinePlayMode;
use slippi_user::UserInfo;

use crate::{with, with_returning};

/// Returns whether the current player is the deciding side of a netplay interaction.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_is_decider(exi_device_instance_ptr: usize) -> bool {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        device.netplay.is_decider
    })
}

/// Instructs the netplay client that a game is starting.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_start_game(exi_device_instance_ptr: usize) {
    with::<SlippiEXIDevice, _>(exi_device_instance_ptr, |device| {
        device.netplay.start_game();
    })
}

/// Instructs the netplay client that a game is starting.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_drop_old_remote_inputs(exi_device_instance_ptr: usize) {
    with::<SlippiEXIDevice, _>(exi_device_instance_ptr, |device| {
        device.netplay.drop_old_remote_inputs();
    })
}

/// C-compatible enum that represents netplay client connection state.
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum SlippiConnectStatus {
    NET_CONNECT_STATUS_UNSET = 0,
    NET_CONNECT_STATUS_INITIATED = 1,
    NET_CONNECT_STATUS_CONNECTED = 2,
    NET_CONNECT_STATUS_FAILED = 3,
    NET_CONNECT_STATUS_DISCONNECTED = 4,
}

/// Returns the current connection status of the netplay client.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_connection_status(exi_device_instance_ptr: usize) -> SlippiConnectStatus {
    with_returning::<SlippiEXIDevice, _, _>(exi_device_instance_ptr, |device| {
        match device.netplay.get_connection_state() {
            NetplayConnectionState::Unset => SlippiConnectStatus::NET_CONNECT_STATUS_UNSET,
            NetplayConnectionState::Initiated => SlippiConnectStatus::NET_CONNECT_STATUS_INITIATED,
            NetplayConnectionState::Connected => SlippiConnectStatus::NET_CONNECT_STATUS_CONNECTED,
            NetplayConnectionState::Failed => SlippiConnectStatus::NET_CONNECT_STATUS_FAILED,
            NetplayConnectionState::Disconnected => SlippiConnectStatus::NET_CONNECT_STATUS_DISCONNECTED
        }
    })
}

#[repr(C)]
pub struct SlippiRemotePadOutput {
    pub latestFrame: c_int,
    pub playerIdx: c_uchar,
    pub data: *mut *mut c_uchar,
    pub dataLen: c_int
}

#[repr(C)]
pub struct SlippiPlayerSelections {
    pub playerIdx: c_uchar,
    pub characterId: c_uchar,
    pub characterColor: c_uchar,
    pub teamId: c_uchar,
    pub isCharacterSelected: bool,
    pub stageId: c_ushort,
    pub isStageSelected: bool,
    pub rngOffset: c_uint,
    pub messageId: c_int,
    pub error: bool
}

/// Update match selections for the current netplay session.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_set_match_selections(
    _exi_device_instance_ptr: usize,
    _selections: SlippiPlayerSelections
) {
    unimplemented!()
}

/// Sends provided packet data across the wire. This method is a stub at the moment, pending
/// some internal API decisions.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_send_async(
    _exi_device_instance_ptr: usize,
    _data: *const u8,
    _len: usize
) {
    unimplemented!()
}

/// A struct that represents player inputs.
#[repr(C)]
pub struct SlippiPad {
    pub frame: c_int,
    pub player_index: c_uchar,
    pub buffer: *mut u8,
    pub buffer_len: c_int
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_send_pad(_exi_device_instance_ptr: usize, _pad: SlippiPad) {
    unimplemented!()
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_remote_sent_chat_message(
    _exi_device_instance_ptr: usize,
    _is_chat_enabled: bool
) -> c_uchar {
    unimplemented!()
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_set_remote_sent_chat_message_id(
    _exi_device_instance_ptr: usize,
    _id: c_uchar
) {
    unimplemented!()
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_remote_chat_message(
    _exi_device_instance_ptr: usize
) -> SlippiPlayerSelections {
    unimplemented!()
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_calc_time_offset_us(_exi_device_instance_ptr: usize) -> c_int {
    unimplemented!()
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_latest_remote_frame(_exi_device_instance_ptr: usize) -> c_int {
    unimplemented!()
}

#[repr(C)]
pub struct SlippiMatchInfo {
    localPlayerSelections: SlippiPlayerSelections,
    remotePlayerSelections: *mut *mut SlippiPlayerSelections,
    remotePlayerSelectionsLen: c_int
}

/// Stubbed for now.
#[unsafe(no_mangle)]
pub extern "C" fn slprs_np_get_match_info(_exi_device_instance_ptr: usize) -> SlippiMatchInfo {
    unimplemented!()
}

// Unsure if we'll even bother or add a different API...
// WriteChatMessageToPacket()
