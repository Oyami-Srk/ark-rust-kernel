use alloc::sync::Arc;
use alloc::vec;
use core::fmt::Write;
use core::ops::DerefMut;
use log::info;
use crate::cpu::CPU;
use crate::device::pipe::PipeFile;
use crate::memory::{VirtAddr, Addr, PageTable, PhyPageId};
use crate::filesystem as fs;
use crate::filesystem::{DirEntry, File, FileModes, FileOpenFlags, InodeStat, SeekPosition};
use crate::process::ProcessData;
use crate::utils::error::EmptyResult;
use crate::syscall::c::*;
use crate::syscall::error::{SyscallError, SyscallResult};

fn get_file_from_fd(proc_data: &ProcessData, fd: usize) -> core::result::Result<Arc<dyn File>, SyscallError> {
    if fd == AT_FDCWD {
        proc_data.cwd.clone()
            .open(FileOpenFlags::O_DIRECTORY | FileOpenFlags::O_RDWR, FileModes::RWX)
            .map_err(|_| SyscallError::ENOENT)
    } else if let Some(Some(file)) = proc_data.files.get(fd) {
        Ok(file.clone())
    } else {
        Err(SyscallError::EBADF)
    }
}

fn get_dentry_from_fd(proc_data: &ProcessData, fd: usize) -> core::result::Result<Arc<DirEntry>, SyscallError> {
    if fd == AT_FDCWD {
        Ok(proc_data.cwd.clone())
    } else if let Some(Some(file)) = proc_data.files.get(fd) {
        file.get_dentry().map_err(|_| SyscallError::ENOENT)
    } else {
        Err(SyscallError::EBADF)
    }
}

/* For Single File */

pub fn open(parent_fd: usize, filename_buf: VirtAddr, flags: FileOpenFlags, mode: FileModes) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();
    let filename = filename_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let cwd = get_dentry_from_fd(&proc_data, parent_fd)?;
    if flags.is_create() {
        todo!("Create todo")
    } else {
        let dentry = DirEntry::from_path(filename, Some(cwd));
        if let Some(dentry) = dentry {
            let file = dentry.open(flags, mode);
            if let Ok(file) = file {
                // find fd
                let fd = proc_data.allocate_fd();
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
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    if let Some(Some(file)) = &proc_data.files.get(fd) {
        if Arc::strong_count(file) > 1 {
            // File is dupped.
            proc_data.files[fd] = None;
            return Ok(0);
        }
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
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();

    let file = get_file_from_fd(&proc_data, fd)?;
    drop(proc_data);

    let mut data = vec![0u8; len];
    if let Ok(read_size) = file.read(data.as_mut_slice()) {
        let data_slice = data.as_slice();
        let proc_data = proc.data.lock();
        // TODO: read more than a page will cause problem...
        // TODO: unwrap is not safe
        let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_u8_mut(read_size);
        phy_buf.copy_from_slice(&data_slice[..read_size]);
        Ok(read_size)
    } else {
        // Err(SyscallError::EIO)
        Ok(0)
    }
}

pub fn write(fd: usize, user_buf: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();

    let file = get_file_from_fd(&proc_data, fd)?;
    let phy_buf = user_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_u8(len);
    drop(proc_data);

    if let Ok(write_size) = file.write(phy_buf) {
        Ok(write_size)
    } else {
        // Err(SyscallError::EIO)
        Ok(0)
    }
}

#[repr(C)]
pub struct IOVec {
    pub iov_base: u64,
    pub iov_len: u64,
}

pub fn readv(fd: usize, io_vecs: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    // Safety: PageTable won't drop since we not return from syscall.
    let page_table = unsafe {
        (proc_data.memory.get_pagetable() as *const PageTable).as_ref().unwrap()
    };
    let file = get_file_from_fd(&proc_data, fd)?;
    drop(proc_data);

    let io_vecs = io_vecs.into_pa(page_table).unwrap().get_slice::<IOVec>(len);
    let mut size = 0;
    for io_vec in io_vecs {
        if io_vec.iov_base == 0 || io_vec.iov_len == 0 {
            continue;
        }
        // TODO: unwrap is not safe
        let buf = VirtAddr::from(io_vec.iov_base as usize).into_pa(page_table).unwrap().get_u8_mut(io_vec.iov_len as usize);
        size += if let Ok(v) = file.read(buf) {
            v
        } else {
            return Err(SyscallError::EIO);
        };
    }
    Ok(size)
}

pub fn writev(fd: usize, io_vecs: VirtAddr, len: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    // Safety: PageTable won't drop since we not return from syscall.
    let page_table = unsafe {
        (proc_data.memory.get_pagetable() as *const PageTable).as_ref().unwrap()
    };
    let file = get_file_from_fd(&proc_data, fd)?;
    drop(proc_data);

    let io_vecs = io_vecs.into_pa(page_table).unwrap().get_slice::<IOVec>(len);
    let mut size = 0;
    for io_vec in io_vecs {
        if io_vec.iov_base == 0 || io_vec.iov_len == 0 {
            continue;
        }
        let buf = VirtAddr::from(io_vec.iov_base as usize).into_pa(page_table).unwrap().get_u8(io_vec.iov_len as usize);
        size += if let Ok(v) = file.write(buf) {
            v
        } else {
            return Err(SyscallError::EIO);
        };
    }
    Ok(size)
}

pub fn lseek(fd: usize, offset: usize, whence: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    let whence = match whence {
        0 => SeekPosition::Set,
        1 => SeekPosition::Cur,
        2 => SeekPosition::End,
        _ => { return Err(SyscallError::ESPIPE); }
    };
    let file = get_file_from_fd(&proc_data, fd)?;
    if let Ok(pos) = file.seek(offset as isize, whence) {
        Ok(pos)
    } else {
        Err(SyscallError::ESPIPE)
    }
}

pub fn linkat(old_dirfd: usize, old_path: VirtAddr, new_dirfd: usize, new_path: VirtAddr, flags: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    let old_dir_dentry = get_dentry_from_fd(&proc_data, old_dirfd)?;
    let new_dir_dentry = get_dentry_from_fd(&proc_data, new_dirfd)?;
    let old_file = DirEntry::from_path(
        old_path.into_pa(proc_data.memory.get_pagetable()).unwrap().get_cstr(),
        Some(old_dir_dentry),
    ).ok_or(SyscallError::ENOENT)?;
    let (new_parent, new_filename) = DirEntry::get_parent(
        new_path.into_pa(proc_data.memory.get_pagetable()).unwrap().get_cstr(),
        Some(new_dir_dentry),
    ).ok_or(SyscallError::ENOENT)?;

    if let Some(inode) = old_file.get_inode() {
        let _ = new_parent.link(inode, new_filename).map_err(|_| SyscallError::EPERM)?;
        Ok(0)
    } else {
        // Cannot link vfs entry
        Err(SyscallError::EPERM)
    }
}

/* For Directory */

pub fn mkdirat(dir_fd: usize, path_buf: VirtAddr, mode: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();
    let path = path_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let dentry = get_dentry_from_fd(&proc_data, dir_fd)?;
    let (parent, dir_name) = DirEntry::get_parent(path, Some(dentry)).ok_or(SyscallError::ENOENT)?;
    if let Ok(_) = parent.mkdir(dir_name) {
        Ok(0)
    } else {
        return Err(SyscallError::EIO);
    }
}

/* For Filesystem */

pub fn mount(dev_buf: VirtAddr, mount_point_buf: VirtAddr, filesystem_buf: VirtAddr, flags: usize, data_ptr: VirtAddr) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();
    let dev = dev_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let mount_point = mount_point_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let filesystem = filesystem_buf.into_pa(&proc_data.memory.get_pagetable()).unwrap().get_cstr();

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
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let dentry = get_dentry_from_fd(&proc_data, fd)?;
    let inode = dentry.get_inode();
    let stat = inode.map(|inode| inode.get_stat()).unwrap_or(InodeStat::vfs_inode_stat());

    // TODO: unwrap is not safe.
    let kstat = kstat_buf.into_pa(proc_data.memory.get_pagetable()).unwrap().get_ref_mut::<KernelStat>();
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
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let dentry = get_dentry_from_fd(&proc_data, dir_fd)?;
    let path = path.into_pa(proc_data.memory.get_pagetable()).unwrap().get_cstr();
    let dentry = if let Some(v) = DirEntry::from_path(path, Some(dentry)) {
        v
    } else {
        return Err(SyscallError::ENOENT);
    };
    let inode = dentry.get_inode();
    let stat = inode.map(|inode| inode.get_stat()).unwrap_or(InodeStat::vfs_inode_stat());

    // TODO: unwrap is not safe.
    let kstat = kstat_buf.into_pa(proc_data.memory.get_pagetable()).unwrap().get_ref_mut::<KernelStat>();
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
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let file = get_file_from_fd(&proc_data, fd)?;
    let dentry = file.get_dentry().map_err(|_| SyscallError::ENOENT)?;
    let mut i = file.seek(0, SeekPosition::Cur).unwrap(); // get current offset
    let mut total_read = 0;
    let mut cur = buf;
    let pg_table = proc_data.memory.get_pagetable();
    loop {
        if let Ok(dentry) = dentry.get_child(i) {
            if let Some(dentry) = dentry {
                let dirent64 = DirEnt64::from_dentry(&dentry, i);
                if total_read + dirent64.len() > len {
                    break;
                }
                // TODO: unwrap is not safe.
                let pa = cur.clone().into_pa(pg_table).unwrap();
                if PhyPageId::from(pa.to_offset(dirent64.len() as isize)) != PhyPageId::from(pa) {
                    cur.access_continuously(pg_table, dirent64.len(), |pa| {
                        pa.get_u8_mut(dirent64.len()).copy_from_slice(dirent64.as_slice());
                    });
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

/* For Pipe */
pub fn pipe2(fds: VirtAddr, options: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let (file_read, file_write) = PipeFile::create();
    let fd_read = proc_data.allocate_fd();
    proc_data.files[fd_read] = Some(Arc::new(file_read));
    let fd_write = proc_data.allocate_fd();
    proc_data.files[fd_write] = Some(Arc::new(file_write));

    let ufds = fds.into_pa(proc_data.memory.get_pagetable()).unwrap().get_slice_mut::<u32>(2);
    ufds[0] = fd_read as u32;
    ufds[1] = fd_write as u32;

    Ok(0)
}

/* For dup */
pub fn dup(old_fd: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let file = get_file_from_fd(&proc_data, old_fd)?;
    let new_fd = proc_data.allocate_fd();
    proc_data.files[new_fd] = Some(file);
    Ok(new_fd)
}

pub fn dup3(old_fd: usize, new_fd: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let mut proc_data = proc.data.lock();

    let file = get_file_from_fd(&proc_data, old_fd)?;
    if let Ok(old_file) = get_file_from_fd(&proc_data, new_fd) {
        old_file.close().or_else(|_| Err(SyscallError::EIO))?
    }
    proc_data.files[new_fd] = Some(file);
    Ok(new_fd)
}