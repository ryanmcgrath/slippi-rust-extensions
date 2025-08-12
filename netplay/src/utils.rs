use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

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
    pub fn set(&self, val: bool) {}

    /// Gets the raw boolean value of this `Flag`.
    pub fn get(&self, val: bool) -> bool {
        false
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
}
