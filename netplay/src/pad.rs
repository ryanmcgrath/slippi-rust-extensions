const SLIPPI_PAD_FULL_SIZE: usize = 0xC;
const SLIPPI_PAD_DATA_SIZE: usize = 0x8;

static EMPTY_PAD: [u8; SLIPPI_PAD_FULL_SIZE] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// A struct that represents player inputs.
#[derive(Debug)]
pub struct SlippiPad {
    pub frame: i32,
    pub player_index: u8,
    pub buffer: [u8; SLIPPI_PAD_FULL_SIZE]
}

impl SlippiPad {
    /// Create and return a new `SlippiPad`. This will be an empty pad for
    /// player 0.
    pub fn new(frame: i32) -> Self {
        Self {
            frame,
            player_index: 0,
            buffer: EMPTY_PAD
        }
    }

    /// Create and return a new pad, with some custom buffer data.
    pub fn new_with_data(frame: i32, buffer: &[u8]) -> Self {
        let mut this = Self::new(frame);
        this.buffer.copy_from_slice(buffer);
        this
    }

    /// Create and return a new pad, with some custom buffer data and a player index.
    pub fn new_with_player_and_data(frame: i32, player_index: u8, buffer: &[u8]) -> Self {
        let mut this = Self::new_with_data(frame, buffer);
        this.player_index = player_index;
        this
    }
}
