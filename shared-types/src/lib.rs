//! This crate implements various shared types and helpers that we need access to in
//! multiple modules.

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

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe flag backed by an atomic boolean. This simply offers us
/// a more consistent and concise API for our purposes.
#[derive(Clone, Debug)]
pub struct Flag(Arc<AtomicBool>);

impl Flag {
    /// Initializes and returns a new `Flag`.
    pub fn new(val: bool) -> Self {
        Self(Arc::new(AtomicBool::new(val)))
    }

    /// Sets the value of this `Flag`.
    pub fn set(&self, val: bool) {
        self.0.store(val, Ordering::Release);
    }

    /// Gets the raw boolean value of this `Flag`.
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A thread-safe queue. This currently uses mutexes for access control locks,
/// but the type is extracted out in order to allow this to be refactored to
/// mirror lock-less queue structures used in the C++ version.
#[derive(Clone, Debug)]
pub struct Queue<T>(Arc<Mutex<VecDeque<T>>>);

impl<T> Queue<T> {
    /// Creates and returns a new `Queue<T>`.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(VecDeque::new())))
    }

    pub fn push_front(&self, entry: T) {
        let mut inner = self.0.lock().expect("Failed to lock queue");
        (*inner).push_front(entry);
    }
}
