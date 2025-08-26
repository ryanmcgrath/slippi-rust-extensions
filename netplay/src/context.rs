use std::net::SocketAddr;

/// Information that describes a match from the matchmaking service.
///
/// This is ultimately passed to the Netplay thread to connect and play.
#[derive(Clone, Debug, Default)]
pub struct MatchContext {
    pub id: String,
    pub local_player_index: usize,
    pub players: Vec<Player>,
    pub stages: Vec<Stage>,

    // Only needed on netplay thread technically...
    pub remote_addrs: Vec<SocketAddr>,
    pub is_host: bool,
}

/// Specific rank information that we hold for match contexts.
#[derive(Copy, Clone, Debug, Default, serde::Deserialize)]
pub struct PlayerRank {
    pub rating: f32,

    #[serde(alias = "globalPlacement")]
    pub global_placing: u16,

    #[serde(alias = "regionalPlacement")]
    pub regional_placing: u16,

    #[serde(alias = "updateCount")]
    pub rating_update_count: u32,
}

/// Player metadata that we get for each match.
///
/// Though this has overlap with the `UserInfo` type, we're implementing a separate
/// type since we don't need to hold some of the same fields.
#[derive(Clone, Debug)]
pub struct Player {
    pub uid: String,
    pub display_name: String,
    pub connect_code: String,
    pub chat_messages: Vec<String>,
    pub rank: PlayerRank,
    pub is_bot: bool,
    pub port: usize
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    PokemonStadium,
    YoshisStory,
    Dreamland,
    Battlefield,
    FinalDestination,
    FountainOfDreams
}

impl Stage {
    pub fn to_u16(&self) -> u16 {
        match self {
            Self::PokemonStadium => 0x3,
            Self::YoshisStory => 0x8,
            Self::Dreamland => 0x1C,
            Self::Battlefield => 0x1F,
            Self::FinalDestination => 0x20,
            Self::FountainOfDreams => 0x2
        }
    }
    
    pub fn from(val: u16) -> Option<Self> {
        match val {
            0x3 => Some(Self::PokemonStadium),
            0x8 => Some(Self::YoshisStory),
            0x1C => Some(Self::Dreamland),
            0x1F => Some(Self::Battlefield),
            0x20 => Some(Self::FinalDestination),
            0x2 => Some(Self::FountainOfDreams),
            _i => None
        }
    }
}
