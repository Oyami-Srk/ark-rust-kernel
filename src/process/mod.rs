//! # Process
//!
//! Process control
//! ---
//! Change log:
//!   - 2024/03/18: File created.

mod pid;
mod process;
// User process
mod task;
// kernel task
mod process_memory;
mod condvar;
mod aux_;


use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use lazy_static::lazy_static;
use log::info;
use riscv::asm::wfi;
use riscv::register::mcause::Trap;
use riscv::register::medeleg::clear_supervisor_env_call;
use crate::core::{Spinlock, SpinlockGuard};

pub use process::{Process, ProcessData, ProcessStatus, ProcessManager};
pub use task::{TaskContext};
pub use condvar::Condvar;
pub use pid::Pid;
use crate::cpu::CPU;
use crate::init;
use crate::interrupt::{enable_trap, TrapContext};
use crate::memory::{Addr, PAGE_SIZE, PhyAddr, PhyPage, PTEFlags, VirtAddr, VirtPageId};
pub use task::{context_switch, do_yield};
use crate::filesystem::{File, SeekPosition};

lazy_static! {
    static ref PROCESS_MANAGER: Spinlock<ProcessManager> = Spinlock::new(ProcessManager::new());
}

pub fn get_process_manager() -> &'static Spinlock<ProcessManager> {
    return &PROCESS_MANAGER
}

pub fn init() {
    let mut init_proc = PROCESS_MANAGER.lock().spawn();
    init_proc.load_elf(init::INIT_BINARY);
    info!("Init proc is loaded.");
}

// Worker is running under every cpu
pub fn worker() -> ! {
    loop {
        enable_trap();
        let proc = PROCESS_MANAGER.lock().scheduler();
        if let Some(proc) = proc {
            // Change current proc
            let mut cpu = CPU::get_current().unwrap();
            let current_proc = cpu.get_process();
            if let Some(current_proc) = current_proc {
                current_proc.data.lock().status = ProcessStatus::Ready;
            }
            let mut proc_data = proc.data.lock();
            proc_data.status = ProcessStatus::Running;
            let new_ctx = &proc_data.kernel_task_context as *const TaskContext;
            drop(proc_data);

            cpu.set_process(Some(proc));

            // switch to proc task context
            let cpu_task_context = cpu.get_context_mut();
            drop(cpu);
            // get proc context

            unsafe { context_switch(cpu_task_context, new_ctx); }
        } else {
            wfi();
        }
    }
}