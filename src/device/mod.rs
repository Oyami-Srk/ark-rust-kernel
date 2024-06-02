//! # Device
//!
//! Devices implementations. NOT ARCH-INDEPENDENT.
//! ---
//! Change log:
//!   - 2024/03/14: File created.

#[macro_use]
pub mod console;
pub mod timer;
pub mod virtio;
pub mod pipe;

pub use console::{Console, Write as ConsoleWrite};
use crate::do_init;
use crate::println;

pub fn init() {
    do_init!(
        console,
        timer,
        virtio
    );
}