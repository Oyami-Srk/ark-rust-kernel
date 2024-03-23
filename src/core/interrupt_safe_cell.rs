//! # Interrupt Safe Cell
//!
//! Accessing to cell with interrupt disabled.
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};
use crate::cpu::CPU;

pub struct InterruptSafeCell<T> {
    data: RefCell<T>
}

pub struct InterruptSafeRefMut<'a, T> {
    data: RefMut<'a, T>
}

impl<T> InterruptSafeCell<T> {
    pub fn new(data: T) -> Self {
        unsafe {
            Self {
                data: RefCell::new(data)
            }
        }
    }

    pub fn get(&self) -> InterruptSafeRefMut<'_, T> {
        CPU::get_current().unwrap().push_interrupt();
        InterruptSafeRefMut{ data: self.data.borrow_mut() }
    }
}

unsafe impl<T> Sync for InterruptSafeCell<T> {}

impl<'a, T> Drop for InterruptSafeRefMut<'a, T> {
    fn drop(&mut self) {
        CPU::get_current().unwrap().push_interrupt();
    }
}

impl<'a, T> Deref for InterruptSafeRefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data.deref()
    }
}

impl<'a, T> DerefMut for InterruptSafeRefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.deref_mut()
    }
}