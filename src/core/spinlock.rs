//! # Spinlock
//!
//! Spinlock implementation in no_std environment
//! 该自旋锁本身是原子的，所以可以跨CPU持有。但是不能保证在持有自旋锁后有其他抢占发生
//! ---
//! Change log:
//!   - 2024/03/15: File created.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::hint;
use core::ops::{Deref, DerefMut};

pub struct Spinlock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
    _marker: PhantomData<T>, // Tell compiler we work as T
}

pub struct SpinlockGuard<'a, T: 'a> {
    lock: &'a Spinlock<T>,
}

// Mutex Implementation

impl<T> Spinlock<T> {
    pub fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
            _marker: PhantomData,
        }
    }

    pub fn lock(&self) -> SpinlockGuard<T> {
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            hint::spin_loop();
        }
        SpinlockGuard::new(self)
    }

    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

// Mark our Mutex is Send + Sync
unsafe impl<T> Sync for Spinlock<T> {}

unsafe impl<T> Send for Spinlock<T> {}

// Mutex Guard Implementation
impl<'a, T> SpinlockGuard<'a, T> {
    pub fn new(lock: &'a Spinlock<T>) -> Self {
        Self {
            lock
        }
    }
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.unlock();
    }
}