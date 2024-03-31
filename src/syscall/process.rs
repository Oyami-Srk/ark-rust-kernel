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

const SIGCHLD: usize = 17;

pub fn clone(flags: usize, child_stack: usize) -> usize {
    if flags != SIGCHLD { warn!("syscall clone with flags is not SIGCHLD."); }
    process::fork(child_stack as *const u8)
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
    process::execve(file, proc, argv, env)
}

pub fn exit(code: usize) -> usize {
    0
}