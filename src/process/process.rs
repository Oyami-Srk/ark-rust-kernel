//! # Process
//!
//! ---
//! Change log:
//!   - 2024/03/19: File created.


use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::max;
use core::mem::size_of;
use fdt::standard_nodes::Memory;
use log::{error, info, warn};
use riscv::register::mcause::Trap;
use crate::core::Spinlock;
use crate::cpu::CPU;
use crate::filesystem::{DirEntry, DirEntryType, File, SeekPosition};
use crate::interrupt::{enable_trap, TrapContext, user_trap_returner};
use super::pid::Pid;
use crate::{config, memory};
use crate::config::CLOCK_FREQ;
use crate::memory::{PAGE_SIZE, PageTable, PhyAddr, PhyPage, PhyPageId, PTEFlags, VirtAddr, Addr, VirtPageId};
use crate::process::{do_yield, PROCESS_MANAGER, TaskContext};
use crate::process::aux_ as aux;
use crate::process::aux_::Aux;
use crate::process::condvar::Condvar;
use crate::syscall::{SyscallError, SyscallResult};
use super::process_memory::ProcessMemory;

#[derive(Copy, Clone, PartialEq)]
pub enum ProcessStatus {
    Ready,
    Running,
    Suspend,
    Zombie,
}

pub struct Process {
    pub pid: Pid,
    pub data: Spinlock<ProcessData>,
}

pub struct ProcessData {
    pub status: ProcessStatus,
    pub exit_code: usize,
    pub parent: Option<Weak<Process>>,
    pub children: Vec<Weak<Process>>,
    pub kernel_stack: Vec<PhyPage>,
    // We use kernel_stack to store trap context
    pub kernel_task_context: TaskContext,
    pub memory: ProcessMemory,
    // Files
    pub cwd: Arc<DirEntry>,
    pub files: Vec<Option<Arc<dyn File>>>,
    // Condvars
    pub condvar_waiting_for_exit: Condvar,
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
            exit_code: 0,
            parent: None,
            children: vec![],
            kernel_stack,
            kernel_task_context,
            memory,
            cwd: DirEntry::root(),
            files: Vec::new(),
            condvar_waiting_for_exit: Condvar::new(),
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

    pub fn load_elf(&self, elf_binary: &[u8]) -> Vec<Aux> {
        let mut aux_table: Vec<Aux> = vec![];
        let elf = xmas_elf::ElfFile::new(elf_binary).unwrap();
        let header = elf.header;
        assert_eq!(header.pt1.magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf.");
        let mut proc_data = self.data.lock();
        let memory = &mut proc_data.memory;
        let mut head_va = 0;
        // Load prog header
        for i in 0..elf.header.pt2.ph_count() {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va = VirtAddr::from(ph.virtual_addr() as usize);
                let end_va = start_va.clone().to_offset(ph.mem_size() as isize);
                if head_va == 0 {
                    head_va = start_va.addr;
                }
                let mut flags = PTEFlags::U;
                if ph.flags().is_read() {
                    flags |= PTEFlags::R;
                }
                if ph.flags().is_write() {
                    flags |= PTEFlags::W;
                }
                if ph.flags().is_execute() {
                    flags |= PTEFlags::X;
                }
                // TODO: Use other model to manage user space mapping
                let size = ph.file_size() as usize;
                let offset = ph.offset() as usize % PAGE_SIZE;
                let total_size = size + offset;
                let pg_count = (total_size + PAGE_SIZE - 1) / PAGE_SIZE;
                let data = &elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize];
                let mut max_vpn: VirtPageId = VirtPageId::from(0);

                for pg in 0..pg_count {
                    // FIXME: non-continuous page could cause UK transfer error.
                    let page = PhyPage::alloc();
                    // continuous page is not required for user program.
                    // info!("Copy 0x{:x} to 0x{:x}", ph.virtual_addr(), page.id.id * PAGE_SIZE);

                    let begin = if pg == 0 { 0 } else { PAGE_SIZE * pg - offset };
                    let inpage_offset = if pg == 0 { offset } else { 0 };
                    let end = if pg == pg_count - 1 {
                        size
                    } else {
                        begin + PAGE_SIZE - inpage_offset
                    };
                    page.copy_u8(inpage_offset, &data[begin..end]);
                    let vpn: VirtPageId = VirtAddr::from(ph.virtual_addr() as usize + PAGE_SIZE * pg).into();
                    // info!("Mapped {}", VirtAddr::from(vpn));
                    max_vpn = vpn;
                    memory.map(vpn, page, flags);
                }

                let need_vpn = VirtPageId::from(VirtAddr::from(ph.virtual_addr() as usize + ph.mem_size() as usize));
                if need_vpn > max_vpn {
                    info!("Allocating {} pages for virt memory.", need_vpn.id - max_vpn.id);
                    for pg in 0..need_vpn.id - max_vpn.id {
                        let page = PhyPage::alloc();
                        memory.map(VirtPageId::from(max_vpn.id + 1 + pg), page, flags);
                    }
                }
            }
        }
        // get prog end
        // prog_end, brk, min_brk is point to first not valid byte.
        // eg. prog_end = 0x1fff, then 0x1ffe is last valid byte. prog_end = 0x2000, whole page is not mapped.
        let prog_end = VirtPageId::from(memory.get_mapped_last_page().id + 1).into();
        memory.prog_end = prog_end;
        memory.brk = memory.prog_end;
        memory.min_brk = memory.prog_end;
        // Setup user stack
        memory.increase_user_stack();
        let sp = memory.stack_base.get_addr();
        let ctx = proc_data.get_trap_context();
        ctx.reg[TrapContext::sp] = sp;
        // Setup entry point
        ctx.sepc = elf.header.pt2.entry_point() as usize;
        // Setup aux
        aux_table.push(Aux::new(aux::AT_PHDR, head_va + elf.header.pt2.ph_offset() as usize));
        aux_table.push(Aux::new(aux::AT_PHENT, elf.header.pt2.ph_entry_size() as usize));
        aux_table.push(Aux::new(aux::AT_PHNUM, elf.header.pt2.ph_count() as usize));
        aux_table.push(Aux::new(aux::AT_PAGESZ, PAGE_SIZE));
        aux_table.push(Aux::new(aux::AT_BASE, 0));
        aux_table.push(Aux::new(aux::AT_FLAGS, 0));
        aux_table.push(Aux::new(aux::AT_ENTRY, elf.header.pt2.entry_point() as usize));
        aux_table.push(Aux::new(aux::AT_UID, 0));
        aux_table.push(Aux::new(aux::AT_EUID, 0));
        aux_table.push(Aux::new(aux::AT_GID, 0));
        aux_table.push(Aux::new(aux::AT_EGID, 0));
        aux_table.push(Aux::new(aux::AT_HWCAP, 0x112d));
        aux_table.push(Aux::new(aux::AT_CLKTCK, CLOCK_FREQ));
        aux_table.push(Aux::new(aux::AT_SECURE, 0));
        aux_table
    }

    pub fn execve(&self, file: Arc<dyn File>, argv: Vec<String>, env: Vec<String>) -> usize {
        let file_size = file.seek(0, SeekPosition::End).unwrap();
        file.seek(0, SeekPosition::Set).unwrap();
        let mut binary_vec = vec![0u8; file_size];
        let read_size = file.read(binary_vec.as_mut_slice()).unwrap();
        assert_eq!(read_size, file_size, "Read size not equal to file size.");
        let binary_slice = binary_vec.as_slice();
        let binary_ptr = binary_slice.as_ptr();
        // clear old user space
        self.data.lock().memory.reset();
        // load new
        // TODO: move out stack allocation
        let mut aux_table = self.load_elf(binary_slice);
        // setup argv and env
        let mut proc_data = self.data.lock();
        let context = proc_data.get_trap_context();
        let virt_sp = context.reg[TrapContext::sp];
        let stack_bottom = VirtAddr::from(virt_sp - PAGE_SIZE)
            .into_pa(proc_data.memory.get_pagetable()).unwrap().to_offset(PAGE_SIZE as isize);
        let mut sp = stack_bottom.clone(); // sp always point a 'valid' data if any valid data available.

        /* Push strings */
        let pusher = |s: String, sp: PhyAddr| -> (PhyAddr, *const u8) {
            let len = s.len();
            let sp = sp.to_offset(-(len as isize + 1)).round_down_to(16);
            let mut_slice = sp.get_slice_mut::<u8>(len + 1);
            mut_slice[..len].copy_from_slice(s.as_bytes());
            mut_slice[len] = 0u8;
            let offset = stack_bottom.get_addr() - sp.get_addr();
            (sp, (virt_sp - offset) as *const u8)
        };

        let mut env_table: Vec<*const u8> = vec![];
        for a in env {
            let (new_sp, vptr) = pusher(a, sp);
            sp = new_sp;
            env_table.push(vptr);
        }
        env_table.push(0 as *const u8);
        sp = sp.round_down_to(size_of::<usize>());

        let mut argv_table: Vec<*const u8> = vec![];
        for a in argv {
            let (new_sp, vptr) = pusher(a, sp);
            sp = new_sp;
            argv_table.push(vptr);
        }
        argv_table.push(0 as *const u8);
        sp = sp.round_down_to(size_of::<usize>());

        // aux
        // aux_table.push(Aux::new(aux::AT_RANDOM, 0));
        aux_table.push(Aux::new(aux::AT_EXECFN, argv_table[0] as usize));
        aux_table.push(Aux::new(aux::AT_NULL, 0));

        /* Push pointers tables */
        sp = sp.to_offset(-((size_of::<Aux>() * aux_table.len()) as isize));
        sp.get_slice_mut::<Aux>(aux_table.len()).copy_from_slice(aux_table.as_slice());
        sp = sp.to_offset(-((size_of::<*const u8>() * env_table.len()) as isize));
        sp.get_slice_mut::<*const u8>(env_table.len()).copy_from_slice(env_table.as_slice());
        let envp = virt_sp - (stack_bottom.get_addr() - sp.get_addr());
        sp = sp.to_offset(-((size_of::<*const u8>() * argv_table.len()) as isize));
        sp.get_slice_mut::<*const u8>(argv_table.len()).copy_from_slice(argv_table.as_slice());
        let argv = virt_sp - (stack_bottom.get_addr() - sp.get_addr());

        if false {
            // OmochaOS Type

            // push envp, argv, argc
            sp = sp.to_offset(-(size_of::<*const u8>() as isize));
            *(sp.get_ref_mut::<*const u8>()) = envp as *const u8;
            sp = sp.to_offset(-(size_of::<*const u8>() as isize));
            *(sp.get_ref_mut::<*const u8>()) = argv as *const u8;
            sp = sp.to_offset(-(size_of::<*const u8>() as isize));
            *(sp.get_ref_mut::<*const u8>()) = (argv_table.len() - 1) as *const u8;

            /* stack should be like:
             * |  0x80000000  | <--- Stack base
             * +--------------+
             * | env strings  |
             * |    padding   |
             * | arg strings  |
             * |    padding   |
             * +--------------+
             * |  TODO: auxv table  |
             * |  envp table  |
             * |  argv table  |
             * +--------------+
             * |     envp     |
             * |     argv     |
             * |     argc     |
             * +--------------+
             * |      sp      | <--- user stack top
             */
        } else {
            // Linux Type

            // push envp, argv, argc
            sp = sp.to_offset(-(size_of::<usize>() as isize));
            *(sp.get_ref_mut::<usize>()) = argv_table.len() - 1;

            /* stack should be like:
             * |  0x80000000  | <--- Stack base
             * +--------------+
             * | env strings  |
             * |    padding   |
             * | arg strings  |
             * |    padding   |
             * +--------------+
             * |  auxv table  |
             * |  envp table  |
             * |  argv table  |
             * |     argc     |
             * +--------------+
             * |      sp      | <--- user stack top
             */
        }

        let virt_sp = virt_sp - (stack_bottom.get_addr() - sp.get_addr());

        // setup context
        context.reg[TrapContext::sp] = virt_sp;
        context.reg[TrapContext::a1] = argv;
        context.reg[TrapContext::a2] = envp;
        context.satp = proc_data.memory.get_satp();

        argv_table.len() - 1 // jump to switch with argc as a0
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        info!("Dropping process {}", self.pid.pid());
    }
}

pub struct ProcessManager {
    process_list: BTreeMap<usize, Arc<Process>>,
    previous_scheduled_pid: usize,
}

const WNOHANG: usize = 1;

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            process_list: BTreeMap::new(),
            previous_scheduled_pid: 0,
        }
    }

    pub fn spawn(&mut self) -> Arc<Process> {
        let proc = Arc::new(Process::new());
        // info!("Process at {:x}", proc.as_ref() as *const Process as usize);
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

    pub fn fork(&mut self, parent: Arc<Process>, child_stack: *const u8) -> usize {
        // TODO: child stack is unused.
        let mut parent_data = parent.data.lock();
        let child = self.spawn();
        let mut child_data = child.data.lock();
        child_data.parent = Some(Arc::downgrade(&parent));
        parent_data.children.push(Arc::downgrade(&child));

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

    pub fn exit(&mut self, proc: Arc<Process>, exit_code: usize) {
        info!("Proc {} want to exit.", proc.pid.pid());
        let strong_count = Arc::strong_count(&proc);
        // There is 3 strong count expected: 1 in ProcessManager's BTree Map. 1 in CPU::current_process. 1 in this function.
        assert_eq!(strong_count, 3, "There is some other holding process' strong ref.");
        assert_ne!(proc.pid.pid(), 1, "Init process want exit!");

        // Set process status
        let mut proc_data = proc.data.lock();
        proc_data.status = ProcessStatus::Zombie;
        proc_data.exit_code = exit_code;
        proc_data.memory.reset(); // 尽量清理内存，但是会留下一个页表根页。

        // reparent child to init
        {
            let init_proc = self.process_list.get(&1).expect("No Init Process Found");
            let mut init_proc_data = init_proc.data.lock();
            while let Some(child) = proc_data.children.pop() {
                init_proc_data.children.push(child);
            }
        }

        // wakeup waiting list
        proc_data.condvar_waiting_for_exit.wakeup();
    }

    pub fn wait_for(pm: &Spinlock<ProcessManager>, parent: Arc<Process>, pid: isize, exit_code: &mut usize, option: usize) -> SyscallResult {
        'outer: loop {
            let mut proc_data = parent.data.lock();
            proc_data.children.retain(|p| p.strong_count() > 0);
            if proc_data.children.len() == 0 {
                break 'outer Err(SyscallError::ECHILD); // No child
            }
            for child in &proc_data.children {
                let child = child.upgrade().unwrap();
                let child_data = child.data.lock();
                let mut result = None;
                if child_data.status == ProcessStatus::Zombie {
                    // found one zombie
                    if pid == -1 || pid == child.pid.pid() as isize {
                        // is what we waited for
                        let pid = child.pid.pid() as isize;
                        *exit_code = child_data.exit_code;
                        result = Some(pid);
                    }
                }
                if let Some(pid) = result {
                    drop(child_data);
                    let _ = pm.lock().process_list.remove(&(pid as usize)); // drop process inside btree map
                    assert_eq!(Arc::strong_count(&child), 1, "Still someone holding child...");
                    drop(child);

                    break 'outer Ok(pid as usize);
                }
            }
            if option == WNOHANG {
                break 'outer Ok(0); // no child found, no hang
            } else {
                // no child found, hang
                drop(proc_data);
                if pid > 0{
                    let pm_guard = pm.lock();
                    let waited_child = pm_guard.process_list.get(&(pid as usize));
                    if let Some(waited_child) = waited_child {
                        let waited_child_data = waited_child.data.lock();
                        waited_child_data.condvar_waiting_for_exit.wait();
                    } else {
                        // not found one
                        break 'outer Err(SyscallError::ECHILD);
                    }
                    drop(pm_guard);
                }
                do_yield();
            }
        }
    }
}