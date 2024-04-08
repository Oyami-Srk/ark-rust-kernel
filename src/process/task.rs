//! # Task
//!
//! 切换不同的内核执行任务
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use alloc::sync::Arc;
use core::arch::global_asm;
use log::info;
use crate::core::SpinlockGuard;
use crate::cpu::CPU;
use crate::interrupt::TrapContext;
use crate::process::{Process, ProcessData, ProcessStatus};
global_asm!(include_str!("switch.S"));

#[repr(C)]
pub struct TaskContext {
    ra: usize,
    // 用来切换ctx
    sp: usize,
    s: [usize; 12],
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }

    pub fn with_sp(self, sp: usize) -> Self {
        Self {
            sp,
            ..self
        }
    }

    pub fn with_ra(self, ra: usize) -> Self {
        Self {
            ra,
            ..self
        }
    }
}

extern "C" {
    pub fn context_switch(old: *mut TaskContext, new: *const TaskContext);
}

fn yield_process(mut proc_data: SpinlockGuard<ProcessData>) -> *mut TaskContext {
    match proc_data.status {
        ProcessStatus::Running => {
            proc_data.status = ProcessStatus::Ready;
        }
        _ => {}
    }
    &mut proc_data.kernel_task_context as *mut TaskContext
}

fn _do_yield(old_ctx: *mut TaskContext) {
    let mut cpu = CPU::get_current().unwrap();
    let trap_enabled = cpu.get_trap_enabled();
    let new_ctx = cpu.get_context();
    cpu.set_process(None);
    drop(cpu);
    // info!("Do Yield for process {} at {:x}", proc.pid.pid(), proc.as_ref() as *const crate::process::Process as usize);

    unsafe { context_switch(old_ctx, new_ctx) };

    CPU::get_current().unwrap().set_trap_enabled(trap_enabled);
}

pub fn do_yield() {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    let old_ctx = yield_process(proc_data);
    drop(proc);
    _do_yield(old_ctx)
}

pub fn try_yield() {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    if let Some(proc_data) = proc.data.try_lock() {
        let old_ctx = yield_process(proc_data);
        _do_yield(old_ctx);
    } else {
        // do nothing
    };
}