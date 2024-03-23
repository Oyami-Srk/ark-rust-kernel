//! # Task
//!
//! 切换不同的内核执行任务
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use core::arch::global_asm;
use crate::cpu::CPU;
global_asm!(include_str!("switch.S"));

#[repr(C)]
pub struct TaskContext {
    ra: usize, // 用来切换ctx
    sp: usize,
    s: [usize; 12]
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

pub fn do_yield() {
    let cpu = CPU::get_current().unwrap();
    let trap_enabled = cpu.get_trap_enabled();
    let proc = cpu.get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let old_ctx = &mut proc_data.kernel_task_context as *mut TaskContext;
    let new_ctx = cpu.get_context();
    drop(proc_data); // FIXME: old_ctx outlived with proc_data
    drop(cpu);

    unsafe { context_switch(old_ctx, new_ctx) };

    CPU::get_current().unwrap().set_trap_enabled(trap_enabled);
}