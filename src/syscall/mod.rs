mod id;
mod file;
mod utils;
mod process;
mod custom;
mod memory;
mod dummy;
mod c;
mod error;


use core::any::Any;
use core::option::Option;
use log::{error, info, trace, warn};
use riscv::asm::ebreak;
use riscv::register::medeleg::set_breakpoint;
pub use id::Syscall;
use crate::cpu::CPU;
use crate::memory::{PhyAddr, VirtAddr};

/*
FIXME: 所有的用户空间访问都没有进行限定
 */

macro_rules! do_syscall {
    ($func:path) => { $func() };
    ($func:path, $args:ident, 0) => { $func() };
    ($func:path, $args:ident, 1) => { $func($args[0].into()) };
    ($func:path, $args:ident, 2) => { $func($args[0].into(), $args[1].into()) };
    ($func:path, $args:ident, 3) => { $func($args[0].into(), $args[1].into(), $args[2].into()) };
    ($func:path, $args:ident, 4) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into()) };
    ($func:path, $args:ident, 5) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into(), $args[4].into()) };
    ($func:path, $args:ident, 6) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into(), $args[4].into(), $args[5].into()) };
}

macro_rules! unimp_syscall {
    ($syscall:ident) => {
        {
            // todo!("{:?} is not implemented yet", syscall)
            error!("Unimplemented syscall {:?} called.", $syscall);
            Ok(0)
        }
    }
}

pub fn syscall_handler(syscall: Syscall, args: &[usize; 6]) -> usize {
    trace!("[Syscall] {:?}, args = [{:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x}]",
                            syscall, args[0], args[1], args[2], args[3], args[4], args[5]);
    let ret = match syscall {
        /* Filesystem */
        Syscall::openat => do_syscall!(file::open, args, 4),
        Syscall::read => do_syscall!(file::read, args, 3),
        Syscall::write => do_syscall!(file::write, args, 3),
        Syscall::readv => do_syscall!(file::readv, args, 3),
        Syscall::writev => do_syscall!(file::writev, args, 3),
        Syscall::lseek => do_syscall!(file::lseek, args, 3),
        Syscall::close => do_syscall!(file::close, args, 1),
        Syscall::mkdirat => do_syscall!(file::mkdirat, args, 3),
        Syscall::mount => do_syscall!(file::mount, args, 5),
        Syscall::fstat => do_syscall!(file::fstat, args, 2),
        Syscall::newfstatat => do_syscall!(file::newfstatat, args, 3),
        Syscall::getdents64 => do_syscall!(file::getdents64, args, 3),
        /* Process */
        Syscall::exit => do_syscall!(process::exit, args, 1),
        Syscall::clone => do_syscall!(process::clone, args, 2),
        Syscall::execve => do_syscall!(process::execve, args, 3),
        Syscall::wait4 => do_syscall!(process::wait_for, args, 3),
        Syscall::getpid => do_syscall!(process::getpid, args, 0),
        Syscall::getppid => do_syscall!(process::getppid, args, 0),
        Syscall::sched_yield => do_syscall!(process::yield_, args, 0),
        /* Memory */
        Syscall::brk => do_syscall!(memory::brk, args, 1),
        Syscall::mmap => do_syscall!(memory::mmap, args, 6),
        Syscall::munmap => do_syscall!(memory::munmap, args, 2),
        /* ARK Custom Syscall */
        Syscall::ark_sleep_ticks => do_syscall!(custom::sleep_ticks, args, 1),
        Syscall::ark_breakpoint => do_syscall!(custom::breakpoint, args, 3),
        /* Dummy stub */
        Syscall::getuid => dummy::ret_zero(syscall),
        Syscall::getgid => dummy::ret_zero(syscall),
        Syscall::setuid => dummy::ret_zero(syscall),
        Syscall::setgid => dummy::ret_zero(syscall),
        Syscall::exit_group => dummy::ret_eperm(syscall),
        Syscall::set_tid_address => dummy::ret_eperm(syscall),
        Syscall::ioctl => dummy::ret_zero(syscall),
        Syscall::fcntl64 => dummy::ret_eperm(syscall),
        Syscall::clock_gettime => dummy::ret_eperm(syscall),
        /* Going to be Implemented */
        Syscall::getcwd => unimp_syscall!(syscall),
        Syscall::dup => unimp_syscall!(syscall),
        Syscall::pipe2 => unimp_syscall!(syscall),
        Syscall::uname => unimp_syscall!(syscall),
        Syscall::rt_sigprocmask => unimp_syscall!(syscall),
        /* Not too urgent to be Implemented */
        Syscall::dup3 => unimp_syscall!(syscall),
        Syscall::chdir => unimp_syscall!(syscall),
        Syscall::linkat => unimp_syscall!(syscall),
        Syscall::unlinkat => unimp_syscall!(syscall),
        Syscall::umount2 => unimp_syscall!(syscall),
        Syscall::times => unimp_syscall!(syscall),
        Syscall::gettimeofday => unimp_syscall!(syscall),
        Syscall::nanosleep => unimp_syscall!(syscall),
    };

    match ret {
        Ok(v) => {
            trace!("[Syscall] {:?}, ret = Ok({:#x})", syscall, v);
            v
        }
        Err(e) => {
            trace!("[Syscall] {:?}, ret = Err({:?})", syscall, e);
            (-(e as isize)) as usize
        }
    }
}