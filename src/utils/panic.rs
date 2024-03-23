//! # Panic
//!
//! Panic handler
//! ---
//! Change log:
//!   - 2024/03/14: File created.

use alloc::fmt;
use core::arch::asm;
use core::fmt::Arguments;
use core::hint;
use core::panic::PanicInfo;
use log::error;
use sbi::system_reset::{ResetReason, ResetType};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("======== Kernel Panic ========");
    if let Some(loc) = info.location() {
        error!("Panicked in file {} line {} column {}: {}", loc.file(), loc.line(), loc.column(), info.message().unwrap_or(&fmt::Arguments::new_const(&[])));
    } else {
        error!("Panicked: {}", info.message().unwrap_or(&fmt::Arguments::new_const(&[])));
    }
    error!("==============================");

    unsafe {
        extern "C" {
            fn boot_sp();
        }
        let mut fp: usize;
        let stop = boot_sp as usize;
        asm!("mv {}, s0", out(reg) fp);
        error!("======== RISCV Backtrace ========");
        for i in 0..10 {
            if fp == stop {
                break;
            }
            error!("#{}:ra={:#x}", i, *((fp - 8) as *const usize));
            fp = *((fp - 16) as *const usize);
        }
        error!("=================================");
    }
    for i in 0..10 {
        riscv::asm::delay(0x1000000);
    }

    sbi::system_reset::system_reset(ResetType::Shutdown, ResetReason::SystemFailure);

    loop {
        hint::spin_loop();
    }
}
