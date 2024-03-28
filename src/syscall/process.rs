use log::warn;
use crate::cpu::CPU;
use crate::process;

const SIGCHLD:usize = 17;

pub fn clone(flags: usize, child_stack: usize) -> usize {
    if flags != SIGCHLD { warn!("syscall clone with flags is not SIGCHLD."); }
    process::fork(child_stack as *const u8)
}

pub fn exit(code: usize) -> usize {
    0
}