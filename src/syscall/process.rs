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
use crate::syscall::error::{SyscallError, SyscallResult};

const SIGCHLD: usize = 17;

pub fn clone(flags: usize, child_stack: usize) -> SyscallResult {
    if flags != SIGCHLD { warn!("syscall clone with flags is not SIGCHLD."); }
    let child_pid = get_process_manager().lock().fork(
        CPU::get_current_process().unwrap(),
        child_stack as *const u8);
    do_yield(); // yield parent
    Ok(child_pid)
}

pub fn execve(path: VirtAddr, argv: VirtAddr, envp: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let page_table = &proc_data.memory.get_pagetable();
    fn get_str_vec(vaddr: VirtAddr, page_table: &PageTable) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        if !vaddr.is_null() {
            let mut pa = vaddr.into_pa(page_table).unwrap();
            loop {
                let str_ptr = VirtAddr::from(*pa.get_ref::<usize>());
                if str_ptr.is_null() { break; }
                let str_ptr = str_ptr.into_pa(page_table).unwrap();
                v.push(str_ptr.get_cstr().to_string());
                pa = pa.to_offset(size_of::<*const u8>() as isize);
            }
        }
        v
    }

    let mut argv = get_str_vec(argv, page_table);
    let mut env = get_str_vec(envp, page_table);
    let dentry = if let Some(d) = DirEntry::from_path(path, Some(proc_data.cwd.clone())) {
        d
    } else {
        return Err(SyscallError::ENOENT);
    };
    let fullpath = dentry.fullpath();
    let file = if let Ok(f) = dentry.open(FileOpenFlags::O_RDONLY, FileModes::from_bits(0).unwrap()) {
        f
    } else {
        return Err(SyscallError::EIO);
    };

    // argv.insert(0, fullpath);
    // env.insert(0, "PATH=/:/mnt".into());

    drop(proc_data);
    Ok(proc.execve(file, argv, env))
}

pub fn exit(code: usize) -> SyscallResult {
    get_process_manager().lock().exit(CPU::get_current_process().unwrap(), code);
    do_yield();
    Ok(0) // never used
}

pub fn wait_for(pid: usize, exit_code_buf: VirtAddr, option: usize) -> SyscallResult {
    let pid: isize = pid as isize;
    let proc = CPU::get_current_process().unwrap();
    // TODO: unwrap is not safe
    let exit_code = exit_code_buf.into_pa(proc.data.lock().memory.get_pagetable()).unwrap().get_ref_mut::<usize>();
    ProcessManager::wait_for(get_process_manager(), proc, pid, exit_code, option)
}

pub fn getppid() -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    if let Some(parent) = &proc_data.parent {
        if let Some(parent) = parent.upgrade() {
            Ok(parent.pid.pid())
        } else {
            Ok(0)
        }
    } else {
        Ok(0)
    }
}

pub fn getpid() -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    Ok(proc.pid.pid())
}

pub fn yield_() -> SyscallResult {
    do_yield();
    Ok(0)
}
