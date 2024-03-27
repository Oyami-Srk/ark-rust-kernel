mod id;
mod file;
mod utils;
mod process;

use core::any::Any;
use core::option::Option;
use log::info;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
pub use id::Syscall;
use crate::cpu::CPU;
use crate::memory::{PhyAddr, VirtAddr};

/*
FIXME: 所有的用户空间访问都没有进行限定
 */

impl From<usize> for Syscall {
    fn from(value: usize) -> Self {
        let opt: Option<Syscall> = FromPrimitive::from_usize(value);
        opt.unwrap_or(Syscall::Unknown)
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
        /* File */
        Syscall::openat => do_syscall!(file::open, args, 4),
        Syscall::read => do_syscall!(file::read, args, 3),
        Syscall::write => do_syscall!(file::write, args, 3),
        Syscall::lseek => do_syscall!(file::lseek, args, 3),
        Syscall::close => do_syscall!(file::close, args, 1),
        /* Process */
        Syscall::clone => do_syscall!(process::clone, args, 2),
        /* Unknown & Unimplemented */
        Syscall::Unknown => {
            // TODO: kill process
            panic!("Unknown syscall called.");
        }
        _ => {
            todo!("{:?} is not implemented yet", syscall)
        }
    }
}