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
