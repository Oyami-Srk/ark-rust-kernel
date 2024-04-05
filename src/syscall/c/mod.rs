/*
Libc 兼容层，不应该流出syscall的scope。
 */

use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;
use bitflags::Flags;
use crate::filesystem::{DirEntry, FileModes, InodeStat};
use crate::memory::{Addr, VirtAddr};

pub const AT_FDCWD: usize = (-100isize) as usize;

#[repr(C)]
pub struct KernelStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad1: u64,
    pub st_size: i64,
    pub st_blksize: i32,
    pub __pad2: i32,
    pub st_blocks: i64,
    pub st_atim: Timespec,
    pub st_mtim: Timespec,
    pub st_ctim: Timespec,
    pub __glibc_reserved: [i32; 2],
}

#[repr(C)]
pub struct Timespec {
    pub tv_sec: i64,
    // seconds
    pub tv_nsec: i64, // nanoseconds
}

#[repr(packed)] // size = 19
pub struct DirEnt64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    // pub d_name: [u8; ?];
}

impl DirEnt64 {
    pub const DT_UNKNOWN: u8 = 0;
    pub const DT_FIFO: u8 = 1;
    pub const DT_CHR: u8 = 2;
    pub const DT_DIR: u8 = 4;
    pub const DT_BLK: u8 = 6;
    pub const DT_REG: u8 = 8;
    pub const DT_LNK: u8 = 10;
    pub const DT_SOCK: u8 = 12;
    pub const DT_WHT: u8 = 14;

    pub fn from_dentry(dentry: &DirEntry, iter_off: usize) -> Vec<u8> {
        let stat = dentry.get_inode().map(|v| v.get_stat()).unwrap_or(InodeStat::vfs_inode_stat());
        let name = dentry.name.clone();

        let len = VirtAddr::from(size_of::<Self>() + name.len() + 1).round_up_to(8).get_addr();

        let header = Self {
            d_ino: stat.ino as u64,
            d_off: iter_off as i64,
            d_reclen: len as u16,
            d_type: match FileModes::from_bits(stat.mode as u32).unwrap().mask_file_type() {
                FileModes::SOCKET => Self::DT_SOCK,
                FileModes::LINK => Self::DT_LNK,
                FileModes::REGULAR => Self::DT_REG,
                FileModes::BLK => Self::DT_BLK,
                FileModes::DIRECTORY => Self::DT_DIR,
                FileModes::CHAR => Self::DT_CHR,
                FileModes::FIFO => Self::DT_FIFO,
                _ => Self::DT_UNKNOWN
            },
        };

        let mut bytes = Vec::with_capacity(len);
        bytes.extend_from_slice(unsafe {
            core::slice::from_raw_parts(
                &header as *const Self as *const u8,
                size_of::<Self>(),
            )
        });
        bytes.extend_from_slice(name.as_bytes());
        while bytes.len() < len {
            bytes.push(0);
        }
        bytes
    }
}