//! # Interrupt lock
//!
//! Intrrupt lock implementation in no_std environment
//! 该中断锁本身是类似自旋锁，所以可以跨CPU持有。同时保证在持有期间不会发生中断
//! ---
//! Change log:
//!   - 2024/04/09: File created.

use core::arch::asm;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::hint;
use core::ops::{Deref, DerefMut};
use crate::core::interrupt_safe_cell::InterruptSafeRefMut;
use crate::core::InterruptSafeCell;

pub struct Intrlock<T> {
    lock: AtomicBool,
    // data: UnsafeCell<T>,
    data: InterruptSafeCell<T>,
    _marker: PhantomData<T>, // Tell compiler we work as T
}

pub struct IntrlockGuard<'a, T: 'a> {
    lock: &'a Intrlock<T>,
    data: InterruptSafeRefMut<'a, T>,
}

impl<T> Intrlock<T> {
    pub fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: InterruptSafeCell::new(data),
            _marker: PhantomData,
        }
    }

    pub fn lock(&self) -> IntrlockGuard<T> {
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            hint::spin_loop();
        }
        IntrlockGuard::new(&self, self.data.get())
    }

    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }

    pub fn try_lock(&self) -> Option<IntrlockGuard<T>> {
        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            None
        } else {
            Some(IntrlockGuard::new(&self, self.data.get()))
        }
    }
}

unsafe impl<T> Sync for Intrlock<T> {}

unsafe impl<T> Send for Intrlock<T> {}

impl<'a, T> IntrlockGuard<'a, T> {
    pub fn new(lock: &'a Intrlock<T>,data: InterruptSafeRefMut<'a, T>) -> Self {
        Self {
            data,
            lock
        }
    }
}

impl<'a, T> Deref for IntrlockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.data.deref()
        }
    }
}

impl<'a, T> DerefMut for IntrlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.deref_mut()
    }
}

impl<'a, T> Drop for IntrlockGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.unlock();
    }
}
