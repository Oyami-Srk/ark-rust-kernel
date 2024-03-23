//! # Process
//!
//! ---
//! Change log:
//!   - 2024/03/19: File created.


use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use log::warn;
use riscv::register::mcause::Trap;
use crate::core::Mutex;
use crate::interrupt::{enable_trap, TrapContext, user_trap_returner};
use super::pid::Pid;
use crate::memory;
use crate::memory::{PAGE_SIZE, PageTable, PhyAddr, PhyPage, PhyPageId, PTEFlags, VirtAddr};
use crate::process::TaskContext;

#[derive(Copy, Clone)]
pub enum ProcessStatus {
    Ready,
    Running,
    Suspend,
}

pub struct Process {
    pub pid: Pid,
    pub data: Mutex<ProcessData>,
}

pub struct ProcessData {
    pub status: ProcessStatus,
    pub parent: Option<Weak<Process>>,
    pub children: Vec<Arc<Process>>,
    pub kernel_stack: PhyPage,
    // We use kernel_stack to store trap context
    pub kernel_task_context: TaskContext,
    pub page_table: PageTable,
    // TODO: sbrk use prog end record
    pub pages: Vec<Arc<PhyPage>>,
}

impl ProcessData {
    pub fn get_trap_context(&mut self) -> &'static mut TrapContext {
        PhyAddr::from(self.kernel_stack.id).get_ref_mut::<TrapContext>()
    }
}

impl Process {
    pub fn new() -> Self {
        let pid = Pid::new();
        let kernel_stack = PhyPage::alloc();
        // let kernel_sp = PhyAddr::from(kernel_stack.id).addr + PAGE_SIZE - size_of::<TrapContext>();
        let kernel_sp = PhyAddr::from(kernel_stack.id).addr + PAGE_SIZE;
        let mut page_table = PageTable::new();
        // Set kernel huge table entry
        page_table.map_big(
            VirtAddr::from(0x80000000), PhyAddr::from(0x80000000),
            PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
        );
        let kernel_task_context = TaskContext::new().with_sp(kernel_sp).with_ra(user_trap_returner as usize);
        let user_satp = page_table.to_satp();
        let mut process_data = ProcessData {
            status: ProcessStatus::Ready,
            parent: None,
            children: vec![],
            kernel_stack,
            kernel_task_context,
            page_table,
            pages: vec![],
        };
        let trap_context = process_data.get_trap_context();
        trap_context.kernel_sp = kernel_sp;
        trap_context.satp = user_satp;

        Self {
            pid,
            data: Mutex::new(process_data),
        }
    }
}

pub struct ProcessManager {
    process_list: BTreeMap<usize, Arc<Process>>,
    previous_scheduled_pid: usize,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            process_list: BTreeMap::new(),
            previous_scheduled_pid: 0
        }
    }

    pub fn spawn(&mut self) -> Arc<Process> {
        let proc = Arc::new(Process::new());
        self.process_list.insert(proc.pid.pid(), proc.clone());
        proc
    }

    pub fn scheduler(&mut self) -> Option<Arc<Process>> {
        // 推举下一个Ready但是没Running的进程
        let bebind = self.process_list.iter().filter(|(pid, _)| {
            pid > &&self.previous_scheduled_pid
        }).find_map(|(_, proc)| {
            match proc.data.lock().status {
                ProcessStatus::Ready => Some(proc.clone()),
                _ => None
            }
        });
        let result = if bebind.is_some() {
            bebind
        } else {
            self.process_list.iter().filter(|(pid, _)| {
                pid <= &&self.previous_scheduled_pid
            }).find_map(|(_, proc)| {
                match proc.data.lock().status {
                    ProcessStatus::Ready => Some(proc.clone()),
                    _ => None
                }
            })
        };
        result.map(|proc| {
            self.previous_scheduled_pid = proc.pid.pid();
            proc
        })
    }
}