use alloc::vec;
use core::fmt::Write;
use core::ops::DerefMut;
use log::info;
use crate::cpu::CPU;
use crate::memory::{VirtAddr, Addr};
use crate::filesystem as fs;
use crate::filesystem::{DirEntry, FileModes, FileOpenFlags, SeekPosition};
use num_traits::FromPrimitive;
use crate::utils::error::EmptyResult;
use crate::syscall::c::*;

/* For Single File */

pub fn open(parent_fd: usize, filename_buf: VirtAddr, flags: FileOpenFlags, mode: FileModes) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let filename = filename_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
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
        let dentry = DirEntry::from_path(filename, Some(cwd));
        if let Some(dentry) = dentry {
            let file = dentry.open(flags, mode);
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
        if let Ok(_) = file.close() {
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
    let file = if let Some(file) = &proc_data.files[fd] {
        file.clone()
    } else {
        return -1isize as usize;
    };
    drop(proc_data);

    let mut data = vec![0u8; len];
    if let Ok(read_size) = file.read(data.as_mut_slice()) {
        let data_slice = data.as_slice();
        let proc_data = proc.data.lock();
        // TODO: read more than a page will cause problem...
        let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).get_u8_mut(read_size);
        phy_buf.copy_from_slice(&data_slice[..read_size]);
        read_size
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
        let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).get_u8(len);
        if let Ok(len) = file.write(phy_buf) {
            len
        } else {
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
        let pos = file.seek(offset as isize, whence);
        if let Ok(pos) = pos {
            pos
        } else {
            -1isize as usize
        }
    } else {
        -1isize as usize
    }
}

/* For Directory */

pub fn mkdirat(dir_fd: usize, path_buf: VirtAddr, mode: usize) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let dentry =
        if -100isize as usize == dir_fd {
            proc_data.cwd.clone()
        } else if dir_fd >= proc_data.files.len() {
            return -1isize as usize;
        } else if let Some(dir_file) = &proc_data.files[dir_fd] {
            dir_file.get_dentry()
        } else {
            return -1isize as usize;
        };
    if let Ok(_) = dentry.mkdir(path) {
        0
    } else {
        -2isize as usize
    }
}

/* For Filesystem */

pub fn mount(dev_buf: VirtAddr, mount_point_buf: VirtAddr, filesystem_buf: VirtAddr, flags: usize, data_ptr: VirtAddr) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let dev = dev_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let mount_point = mount_point_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let filesystem = filesystem_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();

    // flags and data is not yet impl.
    let cwd = proc_data.cwd.clone();
    drop(proc_data);

    match fs::mount(Some(cwd), dev, mount_point, filesystem) {
        Ok(_) => { 0 }
        Err(err) => {
            info!("Mounting {} to {} with {} failed: {}", dev, mount_point, filesystem, err);
            -1isize as usize
        }
    }
}

pub fn fstat(fd: usize, kstat_buf: VirtAddr) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let file = if let Some(Some(f)) = proc_data.files.get(fd) {
        f
    } else {
        return -1isize as usize;
    };

    let kstat = kstat_buf.into_pa(proc_data.memory.get_pagetable()).get_ref_mut::<KernelStat>();
    *kstat = KernelStat {
        st_dev: 0,
        st_ino: 0,
        st_mode: 0o777, // for mod
        st_nlink: 1,
        st_uid: 0,
        st_gid: 0,
        st_rdev: 0,
        __pad1: 0,
        st_size: 0,
        st_blksize: 0,
        __pad2: 0,
        st_blocks: 0,
        st_atim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_mtim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_ctim: Timespec { tv_sec: 0, tv_nsec: 0 },
        __glibc_reserved: [0, 0],
    };

    0
}