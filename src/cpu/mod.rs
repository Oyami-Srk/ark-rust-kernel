//! # CPU
//!
//! RISC-V Per CPU Data
//! ---
//! Change log:
//!   - 2024/03/19: File created.

pub mod vendor;

use alloc::sync::Arc;
use lazy_static::lazy_static;
use alloc::vec::Vec;
use core::arch::asm;
use core::cell::RefCell;
use core::ops::{Deref, DerefMut};
use riscv::register::{mhartid, sstatus};
use log::info;
use crate::interrupt::{disable_trap, enable_trap};
use crate::startup;
use crate::process::{Process, TaskContext};
use crate::interrupt::TrapContext;
use crate::core::{Spinlock, SpinlockGuard};
use spin::RwLock;
pub use vendor::{CpuId, VendorId, ArchId, ImplId, CPUID};

pub(super) struct CPU {
    proc: Spinlock<Option<Arc<Process>>>,
    // trap_off_depth: usize,
    // trap_enabled: bool,
    trap_info: Spinlock<(usize, bool)>,
    cpu_context: Spinlock<TaskContext>,
}

lazy_static! {
    static ref CPUS: Vec<CPU> = (|| {
        let fdt = startup::get_boot_fdt();
        info!("Totally {} CPU(s) found.", fdt.cpus().count());
        // info!("CPU Freq: {}", fdt.cpus().find_map(|c| Some(c.clock_frequency())).unwrap());
        (0..fdt.cpus().count()).map(|_| {
            CPU::new()
        }).collect::<Vec<CPU>>()
    })();
}


pub fn init() {
    let _ = CPUS.len();
}

impl CPU {
    pub fn new() -> Self {
        Self {
            proc: Spinlock::new(None),
            trap_info: Spinlock::new((0, false)),
            cpu_context: Spinlock::new(TaskContext::new()),
        }
    }

    pub fn get_current_id() -> usize {
        if CPUS.len() == 1 {
            0
        } else {
            mhartid::read()
        }
    }

    pub fn get_current() -> Option<&'static CPU> {
        CPUS.get(Self::get_current_id())
    }

    pub fn get_count() -> usize {
        CPUS.len()
    }

    pub fn get_process(&self) -> Option<Arc<Process>> {
        let proc_lock = self.proc.lock();
        let proc = proc_lock.clone();
        drop(proc_lock);
        proc
    }

    pub fn get_current_process() -> Option<Arc<Process>> {
        Self::get_current().unwrap().get_process()
    }

    pub fn push_interrupt(&self) {
        let old_sie = sstatus::read().sie();
        disable_trap();
        let mut trap_info = self.trap_info.lock();
        let (mut depth, mut enabled) = *trap_info;
        if depth == 0 {
            enabled = old_sie;
        }
        depth += 1;
        *trap_info = (depth, enabled);
    }

    pub fn pop_interrupt(&self) {
        assert_eq!(sstatus::read().sie(), false, "Pop interrupt with no interrupt disabled.");
        let mut trap_info = self.trap_info.lock();
        let (mut depth, mut enabled) = *trap_info;
        assert_ne!(depth, 0, "Trap depth is 0");
        depth -= 1;
        *trap_info = (depth, enabled);
        drop(trap_info);

        if depth == 0 && enabled {
            enable_trap();
        }
    }

    pub fn set_process(&self, proc: Option<Arc<Process>>) {
        *self.proc.lock() = proc
    }

    pub fn get_context_mut(&self) -> *mut TaskContext {
        let mut cpu_context = self.cpu_context.lock();
        let ctx = cpu_context.deref_mut() as *mut _;
        drop(cpu_context);
        ctx
    }

    pub fn get_context(&self) -> *const TaskContext {
        let mut cpu_context = self.cpu_context.lock();
        let ctx = cpu_context.deref() as *const _;
        drop(cpu_context);
        ctx
    }
    pub fn get_trap_enabled(&self) -> bool {
        self.trap_info.lock().1
    }

    pub fn set_trap_enabled(&self, enabled: bool) {
        let (depth,_) = *self.trap_info.lock();
        *self.trap_info.lock() = (depth, enabled);
    }
}