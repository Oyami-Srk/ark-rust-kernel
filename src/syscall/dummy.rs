use log::warn;
use crate::syscall::error::{SyscallError, SyscallResult};
use super::Syscall;

pub fn ret_zero(syscall: Syscall) -> SyscallResult {
    warn!("Return dummy result \"0\" for syscall {:?}.", syscall);
    Ok(0)
}

pub fn ret_eperm(syscall: Syscall) -> SyscallResult {
    warn!("Return dummy result \"EPERM\" for syscall {:?}.", syscall);
    Err(SyscallError::EPERM)
}
