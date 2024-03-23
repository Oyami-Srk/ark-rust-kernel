//! # Mutex
//!
//! Mutex implementation in no_std environment
//! 这个Mutex本身是原子的，所以可以跨CPU持有。但是不能保证在持有Mutex后有其他抢占发生
//! ---
//! Change log:
//!   - 2024/03/15: File created.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::hint;
use core::ops::{Deref, DerefMut};

pub struct Mutex<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
    _marker: PhantomData<T>, // Tell compiler we work as T
}

pub struct MutexGuard<'a, T: 'a> {
    lock: &'a Mutex<T>,
}

// Mutex Implementation

impl<T> Mutex<T> {
    pub fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            _marker: PhantomData,
        }
    }

    pub fn lock(&self) -> MutexGuard<T> {
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            hint::spin_loop();
        }
        MutexGuard::new(self)
    }

    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

// Mark our Mutex is Send + Sync
unsafe impl<T> Sync for Mutex<T> {}

unsafe impl<T> Send for Mutex<T> {}

// Mutex Guard Implementation
impl<'a, T> MutexGuard<'a, T> {
    pub fn new(lock: &'a Mutex<T>) -> Self {
        Self {
            lock
        }
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.unlock();
    }
}