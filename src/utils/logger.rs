//! # Logger
//!
//! Logger for kernel routine.
//! ---
//! Change log:
//!   - 2024/03/15: File created.

use log::{Level, LevelFilter, Log, Metadata, Record};
use crate::config::{CLOCK_FREQ, MS_PER_SECOND};
use crate::println;

struct Logger;

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let ticks = riscv::register::time::read64();
        let sec = (ticks as usize) / (CLOCK_FREQ);
        let sub_sec = (ticks as usize) % (CLOCK_FREQ);
        println!("[{}.{}][{: <5}] {}", sec, sub_sec, record.level(), record.args());
    }

    fn flush(&self) {
    }
}

pub fn init() {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("Set logger failed.");
    log::set_max_level(match option_env!("LOG_LEVEL") {
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info
    });
}