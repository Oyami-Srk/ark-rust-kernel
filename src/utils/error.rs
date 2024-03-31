use core::any::Any;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};

pub struct KernelError(&'static str);

impl Debug for KernelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Kernel Error: {}", self.0)
    }
}

impl Display for KernelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Kernel Error: {}", self.0)
    }
}

impl Error for KernelError {}


pub type Result<T> = core::result::Result<T, KernelError>;
pub type EmptyResult = Result<()>;

impl From<&'static str> for KernelError {
    fn from(value: &'static str) -> Self {
        Self(value)
    }
}