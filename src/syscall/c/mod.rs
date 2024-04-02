/*
Libc 兼容层，不应该流出syscall的scope。
 */

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
    pub tv_sec: i64,  // seconds
    pub tv_nsec: i64, // nanoseconds
}
