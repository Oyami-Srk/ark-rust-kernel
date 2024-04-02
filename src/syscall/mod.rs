mod id;
mod file;
mod utils;
mod process;
mod custom;
mod c;

use core::any::Any;
use core::option::Option;
use log::{error, info, warn};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use riscv::asm::ebreak;
use riscv::register::medeleg::set_breakpoint;
pub use id::Syscall;
use crate::cpu::CPU;
use crate::memory::{PhyAddr, VirtAddr};

/*
FIXME: 所有的用户空间访问都没有进行限定
 */

impl From<usize> for Syscall {
    fn from(value: usize) -> Self {
        let opt: Option<Syscall> = FromPrimitive::from_usize(value);
        opt.unwrap_or_else(|| {
            warn!("Got a invalid syscall id {}", value);
            Self::Unknown
        })
    }
}

impl From<Syscall> for usize {
    fn from(value: Syscall) -> Self {
        let opt = ToPrimitive::to_usize(&value);
        opt.unwrap_or(usize::MAX)
    }
}

macro_rules! do_syscall {
    ($func:path, $args:ident, 0) => { $func() };
    ($func:path, $args:ident, 1) => { $func($args[0].into()) };
    ($func:path, $args:ident, 2) => { $func($args[0].into(), $args[1].into()) };
    ($func:path, $args:ident, 3) => { $func($args[0].into(), $args[1].into(), $args[2].into()) };
    ($func:path, $args:ident, 4) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into()) };
    ($func:path, $args:ident, 5) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into(), $args[4].into()) };
    ($func:path, $args:ident, 6) => { $func($args[0].into(), $args[1].into(), $args[2].into(), $args[3].into(), $args[4].into(), $args[5].into()) };
}

pub fn syscall_handler(syscall: Syscall, args: &[usize; 6]) -> usize {
    // info!("Syscall {:?} called.", syscall);
    match syscall {
        /* Filesystem */
        Syscall::openat => do_syscall!(file::open, args, 4),
        Syscall::read => do_syscall!(file::read, args, 3),
        Syscall::write => do_syscall!(file::write, args, 3),
        Syscall::lseek => do_syscall!(file::lseek, args, 3),
        Syscall::close => do_syscall!(file::close, args, 1),
        Syscall::mkdirat => do_syscall!(file::mkdirat, args, 3),
        Syscall::mount => do_syscall!(file::mount, args, 5),
        Syscall::fstat => do_syscall!(file::fstat, args, 2),
        /* Process */
        Syscall::exit => do_syscall!(process::exit, args, 1),
        Syscall::clone => do_syscall!(process::clone, args, 2),
        Syscall::execve => do_syscall!(process::execve, args, 3),
        Syscall::wait4 => do_syscall!(process::wait_for, args, 3),
        Syscall::getpid => do_syscall!(process::getpid, args, 0),
        Syscall::getppid => do_syscall!(process::getppid, args, 0),
        Syscall::sched_yield => do_syscall!(process::yield_, args, 0),
        Syscall::brk => do_syscall!(process::brk, args, 1),
        /* Custom */
        Syscall::ark_sleep_ticks => do_syscall!(custom::sleep_ticks, args, 1),
        Syscall::ark_breakpoint => do_syscall!(custom::breakpoint, args, 3),
        /* Unknown & Unimplemented */
        Syscall::Unknown => {
            // TODO: kill process
            error!("Unknown syscall called.");
            -1isize as usize
        }
        _ => {
            todo!("{:?} is not implemented yet", syscall)
        }
    }
}