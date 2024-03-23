//! # Core
//!
//! Core component for kernel, like Environment.
//! Core should keep arch-independent.
//! ---
//! Change log:
//!   - 2024/03/15: File created.

mod mutex;
mod interrupt_safe_cell;

pub use mutex::{Mutex ,MutexGuard};
pub use interrupt_safe_cell::{InterruptSafeCell};

