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

fn fill_proc_test(proc: Arc<Process>, proc_binary: &[u8]) {
    let prog_size = proc_binary.len();
    let page = PhyPage::alloc_many(8);
    let target = page[0].get_ref_mut::<u8>();
    unsafe {
        core::ptr::copy_nonoverlapping(proc_binary.as_ptr(), target, prog_size);
    }
    let mut data = proc.data.lock();
    let memory = &mut data.memory;
    page.into_iter().enumerate().for_each(|(i, page)| {
        memory.map(VirtAddr::from(PAGE_SIZE * i).into(), page, PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::U);
    });
    memory.prog_end = VirtAddr::from(proc_binary.len());
    memory.brk = memory.prog_end;
    memory.increase_user_stack();
    let ctx = data.get_trap_context();
    ctx.reg[TrapContext::sp] = 0x8000_0000;
    drop(data);
}

fn load_from_elf(proc: Arc<Process>, elf_binary: &[u8]) {
    let elf = xmas_elf::ElfFile::new(elf_binary).unwrap();
    let header = elf.header;
    assert_eq!(header.pt1.magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf.");
    let mut proc_data = proc.data.lock();
    let memory = &mut proc_data.memory;
    // Load prog header
    for i in 0..elf.header.pt2.ph_count() {
        let ph = elf.program_header(i).unwrap();
        if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
            let start_va = VirtAddr::from(ph.virtual_addr() as usize);
            let end_va = start_va.clone().to_offset(ph.mem_size() as isize);
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

            for pg in 0..pg_count {
                // FIXME: non-continuous page could cause UK transfer error.
                let page = PhyPage::alloc(); // continuous page is not required for user program.
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
                memory.map(vpn, page, flags);
            }
        }
    }
    // get prog end
    let prog_end = memory.get_mapped_last_page();
    memory.prog_end = prog_end.into();
    memory.brk = memory.prog_end;
    // Setup user stack
    memory.increase_user_stack();
    let ctx = proc_data.get_trap_context();
    ctx.reg[TrapContext::sp] = 0x8000_0000;
    // Setup entry point
    ctx.sepc = elf.header.pt2.entry_point() as usize;
}

pub fn fork(child_stack: *const u8) -> usize {
    let child_pid = PROCESS_MANAGER.lock().fork(child_stack);
    do_yield(); // yield parent
    child_pid
}

pub fn execve(file: Arc<dyn File>, proc: Arc<Process>, argv: Vec<String>, env: Vec<String>) -> usize {
    let file_size = file.seek(0, SeekPosition::End).unwrap();
    file.seek(0, SeekPosition::Set).unwrap();
    let mut binary_vec = vec![0u8; file_size];
    let read_size = file.read(binary_vec.as_mut_slice()).unwrap();
    assert_eq!(read_size, file_size, "Read size not equal to file size.");
    let binary_slice = binary_vec.as_slice();
    let binary_ptr = binary_slice.as_ptr();
    // clear old user space
    proc.data.lock().memory.reset();
    // load new
    load_from_elf(proc.clone(), binary_slice);
    // setup argv and env
    let mut proc_data = proc.data.lock();
    let context = proc_data.get_trap_context();
    let virt_sp = context.reg[TrapContext::sp];
    let stack_bottom = VirtAddr::from(virt_sp - PAGE_SIZE)
        .into_pa(proc_data.memory.get_pagetable()).to_offset(PAGE_SIZE as isize);
    let mut sp = stack_bottom.clone(); // sp always point a 'valid' data if any valid data available.

    let mut env_table: Vec<*const u8> = vec![];
    for a in env {
        let len = a.len();
        sp = sp.to_offset(-1);
        sp.get_slice_mut::<u8>(1)[0] = 0u8;
        sp = sp.to_offset(-(len as isize));
        sp.get_slice_mut::<u8>(len).copy_from_slice(a.as_bytes());
        let offset = stack_bottom.get_addr() - sp.get_addr();
        env_table.push((virt_sp - offset) as *const u8);
    }
    env_table.push(0 as *const u8);
    sp = sp.round_down_to(size_of::<usize>());

    let mut argv_table: Vec<*const u8> = vec![];
    for a in argv {
        let len = a.len();
        sp = sp.to_offset(-1);
        sp.get_slice_mut::<u8>(1)[0] = 0u8;
        sp = sp.to_offset(-(len as isize));
        sp.get_slice_mut::<u8>(len).copy_from_slice(a.as_bytes());
        let offset = stack_bottom.get_addr() - sp.get_addr();
        argv_table.push((virt_sp - offset) as *const u8);
    }
    argv_table.push(0 as *const u8);
    sp = sp.round_down_to(size_of::<usize>());

    sp = sp.to_offset(-((size_of::<*const u8>() * env_table.len()) as isize));
    sp.get_slice_mut::<*const u8>(env_table.len()).copy_from_slice(env_table.as_slice());
    let envp = virt_sp - (stack_bottom.get_addr() - sp.get_addr());
    sp = sp.to_offset(-((size_of::<*const u8>() * argv_table.len()) as isize));
    sp.get_slice_mut::<*const u8>(argv_table.len()).copy_from_slice(argv_table.as_slice());
    let argv = virt_sp - (stack_bottom.get_addr() - sp.get_addr());

    // push envp, argv, argc
    sp = sp.to_offset(-(size_of::<*const u8>() as isize));
    *(sp.get_ref_mut::<*const u8>()) = envp as *const u8;
    sp = sp.to_offset(-(size_of::<*const u8>() as isize));
    *(sp.get_ref_mut::<*const u8>()) = argv as *const u8;
    sp = sp.to_offset(-(size_of::<*const u8>() as isize));
    *(sp.get_ref_mut::<*const u8>()) = (argv_table.len() - 1) as *const u8;

    let virt_sp = virt_sp - (stack_bottom.get_addr() - sp.get_addr());

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

    // setup context
    context.reg[TrapContext::sp] = virt_sp;
    context.reg[TrapContext::a1] = argv;
    context.reg[TrapContext::a2] = envp;
    context.satp = proc_data.memory.get_satp();

    argv_table.len() - 1 // jump to switch with argc as a0
}

pub fn init() {
    // TODO: setup process
    info!("Testing...");

    for binary in init::PROG_BINARIES {
        load_from_elf(PROCESS_MANAGER.lock().spawn(), binary);
    }
    info!("Loaded...");
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