use slippi_shared_types::AtomicStateTransform;

/// Represents the current state of the matchmaking service.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetplayState {
    Idle,
    Initializing,
    Matchmaking,
    OpponentConnecting,
    ConnectionSuccess,
    ErrorEncountered,
}

impl AtomicStateTransform for NetplayState {
    /// What we need this stored as.
    fn to_i8(&self) -> i8 {
        match self {
            Self::Idle => 0,
            Self::Initializing => 1,
            Self::Matchmaking => 2,
            Self::OpponentConnecting => 3,
            Self::ConnectionSuccess => 4,
            Self::ErrorEncountered => 5,
        }
    }

    /// Marshalling it back from an `i8`.
    fn from_i8(value: i8) -> Self {
        match value {
            0 => NetplayState::Idle,
            1 => NetplayState::Initializing,
            2 => NetplayState::Matchmaking,
            3 => NetplayState::OpponentConnecting,
            4 => NetplayState::ConnectionSuccess,
            5 => NetplayState::ErrorEncountered,

            // This should realistically never happen, since we don't call this path
            // from an area where custom values are held; i.e, a `NetplayState` can only set
            // and get known enum variants, not custom values.
            //
            // The only way this could be triggered is by calling this method directly
            // where one should not do so, or if you did something weird like random number
            // generation in `to_i8()`.
            _ => unreachable!()
        }
    }
}
