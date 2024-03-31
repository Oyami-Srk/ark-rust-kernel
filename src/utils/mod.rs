//! # Utils
//!
//! Utilities for kernel development
//! ---
//! Change log:
//!   - 2024/03/13: File created.

#[macro_use]
pub mod print;
pub mod logger;
pub mod error;
mod panic;
mod fixed_bitset;

#[macro_export]
macro_rules! do_init {
    ($($module:tt),*) => (
        $(
            $module::init();
        )*
    );
}

pub fn round_up_to(value: usize, to: usize) -> usize {
    ((value + (to - 1)) & !(to - 1)).into()
}

pub fn round_down_to(value: usize, to: usize) -> usize {
    (value & !(to - 1)).into()
}
