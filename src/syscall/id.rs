use num_derive::{ToPrimitive, FromPrimitive};

#[repr(usize)]
#[derive(ToPrimitive, FromPrimitive, Debug)]
#[allow(non_camel_case_types)]
pub enum Syscall{
    getcwd = 17,
    lseek = 62,
    pipe2 = 59,
    dup = 23,
    dup3 = 24,
    chdir = 49,
    openat = 56,
    close = 57,
    getdents64 = 61,
    read = 63,
    write = 64,
    linkat = 37,
    unlinkat = 35,
    mkdirat = 34,
    umount2 = 39,
    mount = 40,
    fstat = 80,
    clone = 220,
    execve = 221,
    wait4 = 260,
    exit = 93,
    getppid = 173,
    getpid = 172,
    brk = 214,
    munmap = 215,
    mmap = 222,
    times = 153,
    uname = 160,
    sched_yield = 124,
    gettimeofday = 169,
    nanosleep = 101,
    /* Custom syscall */
    ark_sleep_ticks = 1002,
    ark_breakpoint = 20010125,

    Unknown = usize::MAX
}
