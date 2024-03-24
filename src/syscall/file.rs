use core::fmt::Write;
use crate::cpu::CPU;
use crate::memory::VirtAddr;
use crate::{print, println};
use crate::filesystem as fs;
use crate::filesystem::{FileModes, FileOpenFlags, SeekPosition};
use num_traits::FromPrimitive;

const AT_FDCWD: usize = (-100isize) as usize;

pub fn open(parent_fd: usize, filename_buf: VirtAddr, flags: FileOpenFlags, mode: FileModes) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let filename = filename_buf.into_pa(&proc_data.page_table).get_cstr();
    let cwd = if parent_fd == AT_FDCWD {
        (&proc_data.cwd).clone()
    } else {
        if let Some(file) = &proc_data.files[parent_fd] {
            file.get_dentry().clone()
        } else {
            return -2isize as usize;
        }
    };
    if flags.is_create() {
        todo!("Create todo")
    } else {
        let dentry = fs::get_dentry(filename, Some(cwd));
        if let Some(dentry) = dentry {
            let file = fs::open(dentry, flags, mode);
            if let Ok(file) = file {
                // find fd
                let fd = if let Some(fd) = (0..proc_data.files.len()).find(|fd| proc_data.files[*fd].is_none()) {
                    fd
                } else {
                    proc_data.files.push(None);
                    proc_data.files.len() - 1
                };
                proc_data.files[fd] = Some(file);
                fd
            } else {
                -1isize as usize
            }
        } else {
            -1isize as usize
        }
    }
}

pub fn close(fd: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    if fd >= proc_data.files.len() {
        return -1isize as usize;
    }
    if let Some(file) = &proc_data.files[fd] {
        if let Ok(_) = fs::close(file.clone()) {
            proc_data.files[fd] = None;
            0
        } else {
            -1isize as usize
        }
    } else {
        -1isize as usize
    }
}

pub fn read(fd: usize, user_buf: VirtAddr, len: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    if fd >= proc_data.files.len() {
        return -1isize as usize;
    }
    if let Some(file) = &proc_data.files[fd] {
        if let Ok(data) = fs::read(file.clone(), len) {
            let data_slice = data.as_slice();
            let len = data_slice.len();
            let phy_buf = user_buf.into_pa(&proc_data.page_table).get_u8_mut(len);
            phy_buf.copy_from_slice(data_slice);
            len
        }
        else {
            -1isize as usize
        }
    } else {
        -1isize as usize
    }
}

pub fn write(fd: usize, user_buf: VirtAddr, len: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    if fd >= proc_data.files.len() {
        return -1isize as usize;
    }
    if let Some(file) = &proc_data.files[fd] {
        let phy_buf = user_buf.into_pa(&proc_data.page_table).get_u8(len);
        if let Ok(len) = fs::write(file.clone(), phy_buf) {
            len
        }
        else {
            -1isize as usize
        }
    } else {
        -1isize as usize
    }
}

pub fn lseek(fd: usize, offset: usize, whence: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    let whence = if let Some(whence) = FromPrimitive::from_usize(whence) {
        whence
    } else {
        return -1isize as usize;
    };
    if fd >= proc_data.files.len() {
        return -1isize as usize;
    }
    if let Some(file) = &proc_data.files[fd] {
        let pos = fs::lseek(file.clone(), offset, whence);
        if let Ok(pos) = pos {
            pos
        } else {
            -1isize as usize
        }
    } else {
        -1isize as usize
    }
}
