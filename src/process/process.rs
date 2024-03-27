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
use fdt::standard_nodes::Memory;
use log::warn;
use riscv::register::mcause::Trap;
use crate::core::Spinlock;
use crate::cpu::CPU;
use crate::filesystem::{DirEntry, DirEntryType, File, get_root};
use crate::interrupt::{enable_trap, TrapContext, user_trap_returner};
use super::pid::Pid;
use crate::{config, memory};
use crate::memory::{PAGE_SIZE, PageTable, PhyAddr, PhyPage, PhyPageId, PTEFlags, VirtAddr};
use crate::process::{do_yield, PROCESS_MANAGER, TaskContext};
use crate::process::condvar::Condvar;
use super::process_memory::ProcessMemory;

#[derive(Copy, Clone)]
pub enum ProcessStatus {
    Ready,
    Running,
    Suspend,
}

pub struct Process {
    pub pid: Pid,
    pub data: Spinlock<ProcessData>,
}

pub struct ProcessData {
    pub status: ProcessStatus,
    pub parent: Option<Weak<Process>>,
    pub children: Vec<Arc<Process>>,
    pub kernel_stack: Vec<PhyPage>,
    // We use kernel_stack to store trap context
    pub kernel_task_context: TaskContext,
    pub memory: ProcessMemory,
    // Files
    pub cwd: Arc<DirEntry>,
    pub files: Vec<Option<Arc<dyn File>>>,
}

impl ProcessData {
    pub fn get_trap_context(&mut self) -> &'static mut TrapContext {
        PhyAddr::from(self.kernel_stack[0].id).get_ref_mut::<TrapContext>()
    }
}

impl Process {
    pub fn new() -> Self {
        let pid = Pid::new();
        let kernel_stack = PhyPage::alloc_many(config::PROCESS_KERNEL_STACK_SIZE);
        // let kernel_sp = PhyAddr::from(kernel_stack.id).addr + PAGE_SIZE - size_of::<TrapContext>();
        let kernel_sp = PhyAddr::from(kernel_stack[config::PROCESS_KERNEL_STACK_SIZE - 1].id).addr + PAGE_SIZE * config::PROCESS_KERNEL_STACK_SIZE;

        let memory = ProcessMemory::new();

        let kernel_task_context = TaskContext::new().with_sp(kernel_sp).with_ra(user_trap_returner as usize);
        let user_satp = memory.get_satp();

        let mut process_data = ProcessData {
            status: ProcessStatus::Ready,
            parent: None,
            children: vec![],
            kernel_stack,
            kernel_task_context,
            memory,
            cwd: get_root(),
            files: Vec::new(),
        };
        let trap_context = process_data.get_trap_context();
        trap_context.kernel_sp = kernel_sp;
        trap_context.satp = user_satp;
        // setup files
        process_data.files.push(Some(Arc::new(crate::device::console::Stdin)));
        process_data.files.push(Some(Arc::new(crate::device::console::Stdout)));
        process_data.files.push(Some(Arc::new(crate::device::console::Stdout)));

        Self {
            pid,
            data: Spinlock::new(process_data),
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
            previous_scheduled_pid: 0,
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

    pub fn fork(&mut self, child_stack: *const u8) -> usize {
        let parent = CPU::get_current().unwrap().get_process().unwrap();
        // TODO: child stack is unused.
        let mut parent_data = parent.data.lock();
        let child = self.spawn();
        let mut child_data = child.data.lock();
        child_data.parent = Some(Arc::downgrade(&parent));
        parent_data.children.push(child.clone());

        child_data.memory.copy_from(&parent_data.memory, true);
        child_data.status = ProcessStatus::Ready;
        child_data.cwd = parent_data.cwd.clone();
        child_data.get_trap_context().copy_from(parent_data.get_trap_context());
        child_data.get_trap_context().reg[TrapContext::a0] = 0; // child fork's ret

        drop(child_data);
        drop(parent_data);

        // parent process
        child.pid.pid()
    }
}