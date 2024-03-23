//! # CPU
//!
//! RISC-V Per CPU Data
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use alloc::sync::Arc;
use lazy_static::lazy_static;
use alloc::vec::Vec;
use core::arch::asm;
use riscv::register::{mhartid, sstatus};
use log::info;
use crate::interrupt::{disable_trap, enable_trap};
use crate::startup;
use crate::process::{Process, TaskContext};
use crate::interrupt::TrapContext;
use crate::core::{Mutex, MutexGuard};


pub(super) struct CPU {
    proc: Option<Arc<Process>>,
    trap_off_depth: usize,
    trap_enabled: bool,
    cpu_context: TaskContext,
}

lazy_static! {
    static ref CPUS: Vec<Mutex<CPU>> = (|| {
        let fdt = startup::get_boot_fdt();
        info!("Totally {} CPU(s) found.", fdt.cpus().count());
        // info!("CPU Freq: {}", fdt.cpus().find_map(|c| Some(c.clock_frequency())).unwrap());
        (0..fdt.cpus().count()).map(|_| {
            Mutex::new(CPU::new())
        }).collect::<Vec<Mutex<CPU>>>()
    })();
}


pub fn init() {}

impl CPU {
    pub fn new() -> Self {
        Self {
            proc: None,
            trap_off_depth: 0,
            trap_enabled: false,
            cpu_context: TaskContext::new(),
        }
    }

    pub fn get_current() -> Option<MutexGuard<'static, CPU>> {
        let core_id = if CPUS.len() == 1 {
            0
        } else {
            mhartid::read()
        };
        CPUS.get(core_id).map(|v| v.lock())
    }


    pub fn get_process(&self) -> Option<Arc<Process>> {
        self.proc.clone()
    }

    pub fn push_interrupt(&mut self) {
        let old_sie = sstatus::read().sie();
        disable_trap();
        if self.trap_off_depth == 0 {
            self.trap_enabled = old_sie;
        }
        self.trap_off_depth += 1;
    }

    pub fn pop_interrupt(&mut self) {
        assert_eq!(sstatus::read().sie(), false, "Pop interrupt with no interrupt disabled.");
        assert_ne!(self.trap_off_depth, 0, "Trap depth is 0");
        self.trap_off_depth -= 1;
        if self.trap_off_depth == 0 && self.trap_enabled {
            enable_trap();
        }
    }

    pub fn set_process(&mut self, proc: Arc<Process>) {
        self.proc = Some(proc);
    }

    pub fn unset_process(&mut self) {
        self.proc = None;
    }

    pub fn get_context_mut(&mut self) -> *mut TaskContext {
        &mut self.cpu_context as *mut _
    }

    pub fn get_context(&self) -> *const TaskContext {
        &self.cpu_context as *const _
    }

    pub fn get_trap_enabled(&self) -> bool {
        self.trap_enabled
    }

    pub fn set_trap_enabled(&mut self, enabled: bool) {
        self.trap_enabled = enabled;
    }
}