use alloc::vec;
use core::fmt::Write;
use core::ops::DerefMut;
use log::info;
use crate::cpu::CPU;
use crate::memory::{VirtAddr, Addr, PageTable, PhyPageId};
use crate::filesystem as fs;
use crate::filesystem::{DirEntry, FileModes, FileOpenFlags, InodeStat, SeekPosition};
use crate::filesystem::DirEntryType::File;
use crate::utils::error::EmptyResult;
use crate::syscall::c::*;
use crate::syscall::error::{SyscallError, SyscallResult};

/* For Single File */

pub fn open(parent_fd: usize, filename_buf: VirtAddr, flags: FileOpenFlags, mode: FileModes) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let filename = filename_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let cwd = if parent_fd == AT_FDCWD {
        (&proc_data.cwd).clone()
    } else {
        if let Some(file) = &proc_data.files[parent_fd] {
            file.get_dentry().clone()
        } else {
            return Err(SyscallError::ENOENT);
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
                Ok(fd)
            } else {
                Err(SyscallError::EIO)
            }
        } else {
            Err(SyscallError::EIO)
        }
    }
}

pub fn close(fd: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();

    if let Some(Some(file)) = &proc_data.files.get(fd) {
        if let Ok(_) = file.close() {
            proc_data.files[fd] = None;
            Ok(0)
        } else {
            Err(SyscallError::EIO)
        }
    } else {
        Err(SyscallError::EBADF)
    }
}

pub fn read(fd: usize, user_buf: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();

    let file = if let Some(Some(file)) = &proc_data.files.get(fd) {
        file.clone()
    } else {
        return Err(SyscallError::EBADF);
    };
    drop(proc_data);

    let mut data = vec![0u8; len];
    if let Ok(read_size) = file.read(data.as_mut_slice()) {
        let data_slice = data.as_slice();
        let proc_data = proc.data.lock();
        // TODO: read more than a page will cause problem...
        let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).get_u8_mut(read_size);
        phy_buf.copy_from_slice(&data_slice[..read_size]);
        Ok(read_size)
    } else {
        Err(SyscallError::EIO)
    }
}

pub fn write(fd: usize, user_buf: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();

    let file = if let Some(Some(file)) = &proc_data.files.get(fd) {
        file.clone()
    } else {
        return Err(SyscallError::EBADF);
    };
    let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).get_u8(len);
    drop(proc_data);

    if let Ok(write_size) = file.write(phy_buf) {
        Ok(write_size)
    } else {
        Err(SyscallError::EIO)
    }
}

#[repr(C)]
pub struct IOVec {
    pub iov_base: u64,
    pub iov_len: u64,
}

pub fn readv(fd: usize, io_vecs: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    // Safety: PageTable won't drop since we not return from syscall.
    let page_table = unsafe {
        (proc_data.memory.get_pagetable() as *const PageTable).as_ref().unwrap()
    };
    let file = if let Some(Some(file)) = &proc_data.files.get(fd) {
        file.clone()
    } else {
        return Err(SyscallError::EBADF);
    };
    drop(proc_data);

    let io_vecs = io_vecs.into_pa(page_table).get_slice::<IOVec>(len);
    let mut size = 0;
    for io_vec in io_vecs {
        if io_vec.iov_base == 0 || io_vec.iov_len == 0 {
            continue;
        }
        let buf = VirtAddr::from(io_vec.iov_base as usize).into_pa(page_table).get_u8_mut(io_vec.iov_len as usize);
        size += if let Ok(v) = file.read(buf) {
            v
        } else {
            return Err(SyscallError::EIO);
        };
    }
    Ok(size)
}

pub fn writev(fd: usize, io_vecs: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    // Safety: PageTable won't drop since we not return from syscall.
    let page_table = unsafe {
        (proc_data.memory.get_pagetable() as *const PageTable).as_ref().unwrap()
    };
    let file = if let Some(Some(file)) = &proc_data.files.get(fd) {
        file.clone()
    } else {
        return Err(SyscallError::EBADF);
    };
    drop(proc_data);

    let io_vecs = io_vecs.into_pa(page_table).get_slice::<IOVec>(len);
    let mut size = 0;
    for io_vec in io_vecs {
        if io_vec.iov_base == 0 || io_vec.iov_len == 0 {
            continue;
        }
        let buf = VirtAddr::from(io_vec.iov_base as usize).into_pa(page_table).get_u8(io_vec.iov_len as usize);
        size += if let Ok(v) = file.write(buf) {
            v
        } else {
            return Err(SyscallError::EIO);
        };
    }
    Ok(size)
}

pub fn lseek(fd: usize, offset: usize, whence: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    let whence = match whence {
        0 => SeekPosition::Set,
        1 => SeekPosition::Cur,
        2 => SeekPosition::End,
        _ => { return Err(SyscallError::ESPIPE); }
    };
    if let Some(Some(file)) = &proc_data.files.get(fd) {
        let pos = file.seek(offset as isize, whence);
        if let Ok(pos) = pos {
            Ok(pos)
        } else {
            Err(SyscallError::ESPIPE)
        }
    } else {
        Err(SyscallError::EBADF)
    }
}

/* For Directory */

pub fn mkdirat(dir_fd: usize, path_buf: VirtAddr, mode: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let dentry =
        if -100isize as usize == dir_fd {
            proc_data.cwd.clone()
        } else if let Some(Some(dir_file)) = &proc_data.files.get(dir_fd) {
            dir_file.get_dentry()
        } else {
            return Err(SyscallError::EBADF);
        };
    if let Ok(_) = dentry.mkdir(path) {
        Ok(0)
    } else {
        return Err(SyscallError::EIO);
    }
}

/* For Filesystem */

pub fn mount(dev_buf: VirtAddr, mount_point_buf: VirtAddr, filesystem_buf: VirtAddr, flags: usize, data_ptr: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let dev = dev_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let mount_point = mount_point_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();
    let filesystem = filesystem_buf.into_pa(&proc_data.memory.get_pagetable()).get_cstr();

    // flags and data is not yet impl.
    let cwd = proc_data.cwd.clone();
    drop(proc_data);

    match fs::mount(Some(cwd), dev, mount_point, filesystem) {
        Ok(_) => { Ok(0) }
        Err(err) => {
            info!("Mounting {} to {} with {} failed: {}", dev, mount_point, filesystem, err);
            Err(SyscallError::EIO)
        }
    }
}

pub fn fstat(fd: usize, kstat_buf: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();

    let dentry =
        if -100isize as usize == fd {
            proc_data.cwd.clone()
        } else if let Some(Some(dir_file)) = &proc_data.files.get(fd) {
            dir_file.get_dentry()
        } else {
            return Err(SyscallError::EBADF);
        };

    let inode = dentry.get_inode();
    let stat = inode.map(|inode| inode.get_stat()).unwrap_or(InodeStat::vfs_inode_stat());

    let kstat = kstat_buf.into_pa(proc_data.memory.get_pagetable()).get_ref_mut::<KernelStat>();
    *kstat = KernelStat {
        st_dev: 0,
        st_ino: stat.ino as u64,
        st_mode: stat.mode as u32,
        st_nlink: stat.nlink as u32,
        st_uid: 0,
        st_gid: 0,
        st_rdev: 0,
        __pad1: 0,
        st_size: stat.size as i64,
        st_blksize: stat.block_size as i32,
        __pad2: 0,
        st_blocks: ((stat.size + stat.block_size - 1) / stat.block_size) as i64,
        st_atim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_mtim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_ctim: Timespec { tv_sec: 0, tv_nsec: 0 },
        __glibc_reserved: [0, 0],
    };

    Ok(0)
}

pub fn newfstatat(dir_fd: usize, path: VirtAddr, kstat_buf: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();

    let dentry =
        if -100isize as usize == dir_fd {
            proc_data.cwd.clone()
        } else if let Some(Some(dir_file)) = &proc_data.files.get(dir_fd) {
            dir_file.get_dentry()
        } else {
            return Err(SyscallError::EBADF);
        };


    let path_pa = path.into_pa(proc_data.memory.get_pagetable());
    let path = path_pa.get_cstr();
    let dentry = if let Some(v) = DirEntry::from_path(path, Some(dentry)) {
        v
    } else {
        return Err(SyscallError::ENOENT);
    };
    let inode = dentry.get_inode();
    let stat = inode.map(|inode| inode.get_stat()).unwrap_or(InodeStat::vfs_inode_stat());

    let kstat = kstat_buf.into_pa(proc_data.memory.get_pagetable()).get_ref_mut::<KernelStat>();
    *kstat = KernelStat {
        st_dev: 0,
        st_ino: stat.ino as u64,
        st_mode: stat.mode as u32,
        st_nlink: stat.nlink as u32,
        st_uid: 0,
        st_gid: 0,
        st_rdev: 0,
        __pad1: 0,
        st_size: stat.size as i64,
        st_blksize: stat.block_size as i32,
        __pad2: 0,
        st_blocks: if stat.block_size == 0 {
            0
        } else {
            ((stat.size + stat.block_size - 1) / stat.block_size) as i64
        },
        st_atim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_mtim: Timespec { tv_sec: 0, tv_nsec: 0 },
        st_ctim: Timespec { tv_sec: 0, tv_nsec: 0 },
        __glibc_reserved: [0, 0],
    };

    Ok(0)
}

pub fn getdents64(fd: usize, buf: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();

    let file =
        if let Some(Some(dir_file)) = &proc_data.files.get(fd) {
            dir_file
        } else {
            return Err(SyscallError::EBADF);
        };

    let dentry = file.get_dentry();
    let mut i = file.seek(0, SeekPosition::Cur).unwrap(); // get current offset
    let mut total_read = 0;
    let mut cur = buf;
    loop {
        if let Ok(dentry) = dentry.get_child(i) {
            if let Some(dentry) = dentry {
                let dirent64 = DirEnt64::from_dentry(&dentry, i);
                if total_read + dirent64.len() > len {
                    break;
                }
                let pa = cur.clone().into_pa(proc_data.memory.get_pagetable());
                if PhyPageId::from(pa.to_offset(dirent64.len() as isize)) != PhyPageId::from(pa) {
                    todo!("Cross page access.");
                } else {
                    pa.get_u8_mut(dirent64.len()).copy_from_slice(dirent64.as_slice());
                }
                cur = cur.to_offset(dirent64.len() as isize);
                total_read += dirent64.len();
                i += 1;
            } else {
                break;
            }
        } else {
            return Err(SyscallError::EIO);
        }
    }

    file.seek(i as isize, SeekPosition::Set).unwrap();
    Ok(total_read)
}