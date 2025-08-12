/// The different modes that a player could be in.
///
/// Note that this type uses `serde_repr` to ensure we serialize the value (C-style)
/// and not the name itself.
#[derive(Copy, Clone, Debug, serde_repr::Serialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum OnlinePlayMode {
    Ranked = 0,
    Unranked = 1,
    Direct = 2,
    Teams = 3,
}

impl OnlinePlayMode {
    pub fn is_fixed_rules_mode(&self) -> bool {
        match self {
            Self::Ranked | Self::Unranked => true,
            Self::Direct | Self::Teams => false
        }
    }
}
