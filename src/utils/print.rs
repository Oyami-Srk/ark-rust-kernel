//! # Print
//!
//! Macro for print and println
//! ---
//! Change log:
//!   - 2024/03/14: File created.

use crate::device::{Console, ConsoleWrite};
use core::fmt::{Arguments, Write};

pub fn print(args: Arguments) {
    Console.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::utils::print::print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
