//! # Console Abstract
//!
//! Abstract level for console, using sbi in riscv, or uart in x86.
//! ---
//! Change log:
//!   - 2024/03/14: File created.

pub struct Console;

use alloc::sync::Arc;
use alloc::vec::Vec;
pub use core::fmt::{self, Write};
use sbi::legacy::{console_getchar, console_putchar};
use crate::filesystem::{DirEntry, File, SeekPosition};
use crate::print;
use crate::process::do_yield;
use crate::utils::error::{Result, EmptyResult};

impl Write for Console {
    fn write_str(&mut self, string: &str) -> fmt::Result {
        for char in string.bytes() {
            console_putchar(char.into());
        }
        Ok(())
    }
}

pub struct Stdin;

pub struct Stdout;
// struct Stderr;

impl File for Stdin {
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize> {
        Err("You cannot seek a stream.".into())
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut ch: u8;
        loop {
            if let Some(c) = console_getchar() {
                ch = c;
                break;
            } else {
                do_yield();
            }
        }
        assert_ne!(buf.len(), 0, "Cannot read to empty buf.");
        buf[0] = ch;
        Ok(1)
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        // Err("You cannot write to stdin.".into())
        // FIXME: For whatever newlib bug
        print!("{}", core::str::from_utf8(buf).unwrap());
        Ok(buf.len())
    }

    fn close(&self) -> EmptyResult { Ok(())}

    fn get_dentry(&self) -> Arc<DirEntry> {
        panic!("Invalid get dentry for stdin/stdout")
    }
}

impl File for Stdout {
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize> {
        Err("You cannot seek a stream.".into())
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        Err("You cannot read from stdout.".into())
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        print!("{}", core::str::from_utf8(buf).unwrap());
        Ok(buf.len())
    }

    fn close(&self) -> EmptyResult { Ok(()) }

    fn get_dentry(&self) -> Arc<DirEntry> {
        panic!("Invalid get dentry for stdin/stdout")
    }
}

pub fn init() {}