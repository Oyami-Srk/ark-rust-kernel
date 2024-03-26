//! # main
//!
//! Entrance of kernel.
//! ---
//! Change log:
//!   - 2024/03/13: File created.

#![no_main]
#![no_std]
#![feature(panic_info_message, fmt_internals, strict_provenance, error_in_core, macro_metavar_expr)]
#![feature(let_chains)]

#![allow(dead_code)] // Development only
#![allow(warnings)]

extern crate alloc;

mod startup;
mod cpu;
mod utils;
mod core;
mod memory;
mod device;
mod init;
mod process;
mod interrupt;
mod syscall;
mod filesystem;

use alloc::vec;
use alloc::vec::Vec;
use utils::logger;
use log::info;

use sbi::system_reset::{ResetReason, ResetType};
use crate::interrupt::enable_trap;
use crate::memory::PhyPage;

pub fn main(core_id: u64, boot_core: bool) -> ! {
    if ! boot_core {
        info!("Slave core startup.");
        interrupt::init();
        process::worker();
    }

    println!(r#"
    ___         __       ____             __      __ __                     __
   /   |  _____/ /__    / __ \__  _______/ /_    / //_/__  _________  ___  / /
  / /| | / ___/ //_/   / /_/ / / / / ___/ __/   / ,< / _ \/ ___/ __ \/ _ \/ /
 / ___ |/ /  / ,<     / _, _/ /_/ (__  ) /_    / /| /  __/ /  / / / /  __/ /
/_/  |_/_/  /_/|_|   /_/ |_|\__,_/____/\__/   /_/ |_\___/_/  /_/ /_/\___/_/
Ark Rust Kernel ({}), Created by Shiroko, with love and passion.
"#, env!("CARGO_PKG_VERSION"));

    do_init!(
        logger,
        interrupt,
        memory,
        startup,
        filesystem,
        device,
        process
    );

    process::worker()
}