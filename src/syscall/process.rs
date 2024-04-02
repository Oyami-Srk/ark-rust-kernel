use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use bitflags::Flags;
use log::warn;
use crate::cpu::CPU;
use crate::filesystem::{DirEntry, FileModes, FileOpenFlags};
use crate::memory::{Addr, PageTable, PhyAddr, VirtAddr};
use crate::process;
use crate::process::{do_yield, get_process_manager, ProcessManager};

const SIGCHLD: usize = 17;

pub fn clone(flags: usize, child_stack: usize) -> usize {
    if flags != SIGCHLD { warn!("syscall clone with flags is not SIGCHLD."); }
    let child_pid = get_process_manager().lock().fork(CPU::get_current().unwrap().get_process().unwrap(), child_stack as *const u8);
    do_yield(); // yield parent
    child_pid
}

pub fn execve(path: VirtAddr, argv: VirtAddr, envp: VirtAddr) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let page_table = &proc_data.memory.get_pagetable();
    fn get_str_vec(vaddr: VirtAddr, page_table: &PageTable) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        if !vaddr.is_null() {
            let mut pa = vaddr.into_pa(page_table);
            loop {
                let str_ptr = VirtAddr::from(*pa.get_ref::<usize>()).into_pa(page_table);
                if str_ptr.is_null() { break; }
                v.push(str_ptr.get_cstr().to_string());
                pa = pa.to_offset(size_of::<*const u8>() as isize);
            }
        }
        v
    }

    let argv = get_str_vec(argv, page_table);
    let env = get_str_vec(envp, page_table);
    let dentry = if let Some(d) = DirEntry::from_path(path, Some(proc_data.cwd.clone())) {
        d
    } else {
        return -1isize as usize;
    };
    let file = if let Ok(f) = dentry.open(FileOpenFlags::ReadOnly, FileModes::from_bits(0).unwrap()) {
        f
    } else {
        return -2isize as usize;
    };

    drop(proc_data);
    proc.execve(file, argv, env)
}

pub fn exit(code: usize) -> usize {
    get_process_manager().lock().exit(CPU::get_current().unwrap().get_process().unwrap(), code);
    do_yield();
    0 // never used
}

pub fn wait_for(pid: usize, exit_code_buf: VirtAddr, option: usize) -> usize {
    let pid: isize = pid as isize;
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let exit_code = exit_code_buf.into_pa(proc.data.lock().memory.get_pagetable()).get_ref_mut::<usize>();
    ProcessManager::wait_for(get_process_manager(), proc, pid, exit_code, option) as usize
}

pub fn getppid() -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    if let Some(parent) = &proc_data.parent {
        if let Some(parent) = parent.upgrade() {
            parent.pid.pid()
        } else {
            0
        }
    } else {
        0
    }
}

pub fn getpid() -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    proc.pid.pid()
}

pub fn yield_() -> usize {
    do_yield();
    0
}

pub fn brk(addr: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    proc_data.memory.set_brk(addr.into())
}