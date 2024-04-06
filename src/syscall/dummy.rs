use log::{trace, warn};
use crate::syscall::error::{SyscallError, SyscallResult};
use super::Syscall;

pub fn ret_zero(syscall: Syscall) -> SyscallResult {
    trace!("Return dummy result \"0\" for syscall {:?}.", syscall);
    Ok(0)
}

pub fn ret_eperm(syscall: Syscall) -> SyscallResult {
    trace!("Return dummy result \"EPERM\" for syscall {:?}.", syscall);
    Err(SyscallError::EPERM)
}

pub fn unimp(syscall: Syscall) -> SyscallResult {
    trace!("Unimplemented syscall {:?} called.", syscall);
    Err(SyscallError::EPERM)
}