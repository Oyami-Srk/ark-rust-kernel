use core::mem::size_of;
use crate::cpu::CPU;
use crate::filesystem::DirEntry;
use crate::memory::{Addr, VirtAddr};
use crate::syscall::c::UtsName;
use crate::syscall::error::{SyscallError, SyscallResult};

pub fn uname(buf: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    // TODO: unwrap is not safe
    let buf = buf.into_pa(proc_data.memory.get_pagetable()).unwrap();

    let uname = UtsName::new();
    buf.get_u8_mut(size_of::<UtsName>()).copy_from_slice(uname.as_bytes());

    Ok(0)
}

pub fn getcwd(buf: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let mut fullpath_of_cwd = proc_data.cwd.fullpath();
    fullpath_of_cwd.push('\0');
    let fullpath_of_cwd_bytes = fullpath_of_cwd.as_bytes();
    if fullpath_of_cwd_bytes.len() > len {
        Err(SyscallError::ENOMEM)
    } else {
        if buf.is_null() {
            todo!("Allocating cwd path buf by kernel.")
        } else {
            if let Some(buf_pa) = proc_data.memory.translate_with_stack_alloc(buf) {
                buf_pa.get_u8_mut(fullpath_of_cwd_bytes.len())
                    .copy_from_slice(fullpath_of_cwd_bytes);
                Ok(buf.get_addr())
            } else {
                Err(SyscallError::ENOMEM)
            }
        }
    }
}

pub fn chdir(path: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path.into_pa(proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let new_cwd = DirEntry::from_path(path, Some(proc_data.cwd.clone()));
    if let Some(new_cwd) = new_cwd {
        proc_data.cwd = new_cwd;
        Ok(0)
    } else {
        Err(SyscallError::EPERM)
    }
}