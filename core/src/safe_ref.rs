#[cfg(not(feature = "parallel"))]
use core::cell::RefCell as Cell;

#[cfg(feature = "parallel")]
use std::sync::Mutex as Cell;

#[cfg(not(feature = "parallel"))]
pub use core::cell::RefMut as Lock;

#[cfg(not(feature = "parallel"))]
pub use alloc::rc::{Rc as Ref, Weak};

#[cfg(feature = "parallel")]
pub use std::sync::{Arc as Ref, MutexGuard as Lock, Weak};

#[repr(transparent)]
pub struct Mut<T: ?Sized>(Cell<T>);

impl<T> Mut<T> {
    pub fn new(inner: T) -> Self {
        Self(Cell::new(inner))
    }
}

impl<T: Default> Default for Mut<T> {
    fn default() -> Self {
        Mut::new(T::default())
    }
}

impl<T: ?Sized> Mut<T> {
    pub fn lock(&self) -> Lock<T> {
        #[cfg(not(feature = "parallel"))]
        {
            self.0.borrow_mut()
        }

        #[cfg(feature = "parallel")]
        {
            // A panic caught across the FFI boundary must not poison the
            // runtime for later calls: the stored panic is resumed by the
            // caller, and the protected data stays consistent.
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }

    pub fn try_lock(&self) -> Option<Lock<T>> {
        #[cfg(not(feature = "parallel"))]
        {
            self.0.try_borrow_mut().ok()
        }

        #[cfg(feature = "parallel")]
        {
            match self.0.try_lock() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => None,
            }
        }
    }
}
