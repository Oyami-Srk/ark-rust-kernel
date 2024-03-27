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


use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use log::info;
use riscv::asm::wfi;
use crate::core::Spinlock;

pub use process::{Process, ProcessData, ProcessStatus, ProcessManager};
pub use task::{TaskContext};
pub use pid::Pid;
use crate::cpu::CPU;
use crate::init;
use crate::interrupt::{enable_trap, TrapContext};
use crate::memory::{Addr, PAGE_SIZE, PhyAddr, PhyPage, PTEFlags, VirtAddr, VirtPageId};
pub use task::{context_switch, do_yield};

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