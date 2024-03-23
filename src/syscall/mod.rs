mod io;

use core::any::Any;
use core::option::Option;
use log::info;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};



#[repr(usize)]
#[derive(ToPrimitive, FromPrimitive, Debug)]
pub enum Syscall{
    Write = 64,
    Exit = 93,
    Unknown = usize::MAX
}

impl From<usize> for Syscall {
    fn from(value: usize) -> Self {
        let opt:Option<Syscall> = FromPrimitive::from_usize(value);
        opt.unwrap_or(Syscall::Unknown)
    }
}

impl From<Syscall> for usize {
    fn from(value: Syscall) -> Self {
        let opt = ToPrimitive::to_usize(&value);
        opt.unwrap_or(usize::MAX)
    }
}


pub fn syscall_handler(syscall: Syscall, args: &[usize; 6]) -> usize {
    // info!("Syscall {:?} called.", syscall);
    match syscall {
        Syscall::Write => io::write(args),
        Syscall::Exit => 0,
        Syscall::Unknown => {
            // TODO: kill process
            panic!("Unknown syscall called.");
        }
    }
}