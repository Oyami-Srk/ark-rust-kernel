//! # Console Abstract
//!
//! Abstract level for console, using sbi in riscv, or uart in x86.
//! ---
//! Change log:
//!   - 2024/03/14: File created.

pub struct Console;
pub use core::fmt::{self, Write};
use sbi::legacy::console_putchar;

impl Write for Console {
    fn write_str(&mut self, string: &str) -> fmt::Result {
        for char in string.bytes() {
            console_putchar(char.into());
        }
        Ok(())
    }
}
