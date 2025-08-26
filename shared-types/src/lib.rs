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
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};

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

use std::sync::OnceLock;

/// A lock-free thread-safe value that can be set exactly once.
///
/// This is useful for situations where we have background thread(s) that might
/// need to update a string that the main (game) thread can reliably read without
/// locking (the read-path of this value is effectively lock-free via OnceLock).
#[derive(Clone, Debug)]
pub struct OnceValue<T>(Arc<OnceLock<T>>);

impl<T> OnceValue<T> {
    /// Initializes a new `OnceValue`.
    pub fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    /// Sets the underlying value.
    pub fn set(&self, value: T) {
        if let Err(_value) = self.0.set(value) {
            // We can't import the dolphin crate here to target, but this will at least still 
            // show up in generic output logs thanks to tracing.
            //
            // It should also never be hit, but stranger things have happened.
            tracing::warn!("OnceValue double set, will drop value");
        }
    }

    /// Gets a reference to the underlying value.
    ///
    /// For our purposes, we shuffle the `None` case to a blank string value.
    pub fn get(&self) -> Option<&T> {
        self.0.get()
    }
}

/// Types can implement this in order to be used with `AtomicState`.
pub trait AtomicStateTransform {
    /// Convert the value to an `i8` representation.
    fn to_i8(&self) -> i8;

    /// Map an `i8` to a value.
    ///
    /// Implementing types might consider using `std::unreachable!()` for
    /// match arms that can never be hit.
    fn from_i8(value: i8) -> Self;
}

/// A thread safe state marker that uses atomics rather than any form of mutex.
///
/// Internally, the held type must implement `AtomicStateTransform`; the values
/// must map cleanly to an `i8`. If you require a state that holds its own state,
/// then you probably want a mutex.
#[derive(Clone, Debug)]
pub struct AtomicState<T> {
    inner: Arc<AtomicI8>,
    marker: std::marker::PhantomData<T>
}   

impl<T> AtomicState<T>
where
    T: AtomicStateTransform,
{
    /// Initializes a new `AtomicState`.
    pub fn new(state: T) -> Self {
        Self {
            inner: Arc::new(AtomicI8::new(state.to_i8())),
            marker: std::marker::PhantomData
        }
    }

    /// Sets the underlying value of this state.
    pub fn set(&self, state: T) {
        self.inner.store(state.to_i8(), Ordering::Release);
    }

    /// Gets the undetlying value of this state.
    pub fn get(&self) -> T {
        let value = self.inner.load(Ordering::Relaxed);
        T::from_i8(value)
    }
}
