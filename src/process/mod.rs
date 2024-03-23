//! # Process
//!
//! Process control
//! ---
//! Change log:
//!   - 2024/03/18: File created.

mod pid;
mod process;
// User process
mod task;       // kernel task

use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use log::info;
use riscv::asm::wfi;
use crate::core::Mutex;

pub use process::{Process, ProcessData, ProcessStatus, ProcessManager};
pub use task::{TaskContext};
pub use pid::Pid;
use crate::cpu::CPU;
use crate::init;
use crate::interrupt::enable_trap;
use crate::memory::{PAGE_SIZE, PhyAddr, PhyPage, PTEFlags, VirtAddr};
pub use task::{context_switch, do_yield};

lazy_static! {
    static ref PROCESS_MANAGER: Mutex<ProcessManager> = Mutex::new(ProcessManager::new());
}

fn fill_proc_test(proc: Arc<Process>, proc_binary: &[u8]) {
    let prog_size = proc_binary.len();
    let page = PhyPage::alloc_many(8);
    let target = page[0].get_ref_mut::<u8>();
    unsafe {
        core::ptr::copy_nonoverlapping(proc_binary.as_ptr(), target, prog_size);
    }
    let mut data = proc.data.lock();
    for i in 0..8 {
        data.page_table.map(VirtAddr::from(PAGE_SIZE * i), PhyAddr::from(page[i].id), PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::U);
    }
    let ctx = data.get_trap_context();
    ctx.reg[2] = PAGE_SIZE * 8;
    let page = page.into_iter().map(|p| Arc::new(p)).into_iter();
    data.pages.extend(page);
    let new_ctx = &data.kernel_task_context as *const TaskContext;
    drop(data);
}

pub fn init() {
    // TODO: setup process
    info!("Testing...");

    for binary in init::PROG_BINARIES {
        fill_proc_test(PROCESS_MANAGER.lock().spawn(), binary);
    }
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

            cpu.set_process(proc);

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
