//! # Pid
//!
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use alloc::vec::Vec;
use lazy_static::lazy_static;
use crate::core::Mutex;

#[derive(PartialEq)]
pub struct Pid(usize);

pub struct RecycleAllocator {
    max: usize,
    free: Vec<usize>,
}

impl RecycleAllocator {
    pub fn new() -> Self {
        RecycleAllocator {
            max: 0,
            free: Vec::new(),
        }
    }
    pub fn alloc(&mut self) -> usize {
        if let Some(id) = self.free.pop() {
            id
        } else {
            self.max += 1;
            self.max
        }
    }
    pub fn free(&mut self, id: usize) {
        assert!(id <= self.max);
        assert!(!self.free.iter().any(|i| *i == id), "{} already freed.", id);
        self.free.push(id);
    }
}

lazy_static! {
    static ref PID_ALLOCATOR: Mutex<RecycleAllocator> = Mutex::new(RecycleAllocator::new());
}

impl Pid {
    pub fn new() -> Self {
        Self(PID_ALLOCATOR.lock().alloc())
    }
    pub fn pid(&self) -> usize { self.0 }
}

impl Drop for Pid {
    fn drop(&mut self) {
        PID_ALLOCATOR.lock().free(self.0);
    }
}