//! # Address
//!
//! Implementation of idiomatic address for Rust
//! ---
//! Change log:
//!   - 2024/03/17: File created.

use core::fmt::{Display, Formatter};
use core::ops::{Add, Sub};
use crate::memory::PAGE_SIZE;

// Declarations
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct PhyAddr {
    pub addr: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct VirtAddr {
    pub addr: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct PhyPageId {
    pub id: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct VirtPageId {
    pub id: usize,
}

// Implementations for physical part
impl From<usize> for PhyAddr {
    fn from(addr: usize) -> Self {
        Self {
            addr
        }
    }
}

impl From<PhyPageId> for PhyAddr {
    fn from(value: PhyPageId) -> Self {
        Self {
            addr: value.id * PAGE_SIZE
        }
    }
}

impl From<usize> for PhyPageId {
    fn from(value: usize) -> Self {
        Self {
            id: value
        }
    }
}

impl From<PhyAddr> for PhyPageId {
    fn from(value: PhyAddr) -> Self {
        Self {
            id: value.addr / PAGE_SIZE
        }
    }
}

impl Add<usize> for PhyPageId {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self {
            id: self.id + rhs
        }
    }
}

impl Sub<usize> for PhyPageId {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        if rhs > self.id {
            Self {
                id: 0
            }
        } else {
            Self {
                id: self.id - rhs
            }
        }
    }
}

// Virt part
impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self {
            addr
        }
    }
}

impl From<VirtPageId> for VirtAddr {
    fn from(value: VirtPageId) -> Self {
        Self {
            addr: value.id * PAGE_SIZE
        }
    }
}

impl From<usize> for VirtPageId {
    fn from(value: usize) -> Self {
        Self {
            id: value
        }
    }
}

impl From<VirtAddr> for VirtPageId {
    fn from(value: VirtAddr) -> Self {
        Self {
            id: value.addr / PAGE_SIZE
        }
    }
}

// Implementation of Display
impl Display for PhyAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[PhyAddr: {:#x}]", &self.addr))
    }
}

impl Display for PhyPageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[PhyPageId: {}]", &self.id))
    }
}

impl Display for VirtAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[VirtAddr: {:#x}]", &self.addr))
    }
}

impl Display for VirtPageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[VirtPageId: {}]", &self.id))
    }
}

pub trait Addr: Sized + From<usize> {
    fn get_addr(&self) -> usize;

    fn round_up_to(&self, to: usize) -> Self {
        ((self.get_addr() + (to - 1)) & !(to - 1)).into()
    }

    fn round_down_to(&self, to: usize) -> Self {
        (self.get_addr() & !(to - 1)).into()
    }

    fn round_up(&self) -> Self {
        self.round_up_to(PAGE_SIZE)
    }

    fn round_down(&self) -> Self {
        self.round_down_to(PAGE_SIZE)
    }

    fn to_offset(self, offset: isize) -> Self {
        let addr = if offset < 0 {
            if self.get_addr() < (offset.unsigned_abs()) {
                0
            } else {
                self.get_addr() - offset.unsigned_abs()
            }
        } else {
            self.get_addr() + offset.unsigned_abs()
        };
        Self::from(addr)
    }
}

impl PhyAddr {
    /* Unsafe wrapper */
    pub fn get_ref<T>(&self) -> &'static T {
        // Reference about a address is live forever.
        unsafe { (self.addr as *const T).as_ref().expect("Try to get reference to null") }
    }

    pub fn get_ref_mut<T>(&self) -> &'static mut T {
        unsafe { (self.addr as *mut T).as_mut().expect("Try to get mutable reference to null") }
    }

    pub fn get_slice_mut<T>(&self, len: usize) -> &'static mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.addr as *mut T, len) }
    }
}

impl Addr for PhyAddr {
    fn get_addr(&self) -> usize {
        self.addr
    }
}

impl PhyPageId {}

impl VirtAddr {}

impl Addr for VirtAddr {
    fn get_addr(&self) -> usize {
        self.addr
    }
}

impl VirtPageId {
    pub fn get_pte_indexes(&self) -> [usize; 3] {
        let mut vpn = self.id;
        let mut idx = [0usize; 3];
        for i in (0..3).rev() {
            idx[i] = vpn & 511;
            vpn >>= 9;
        }
        idx
    }
}