//! # Core
//!
//! Core component for kernel, like Environment.
//! Core should keep arch-independent.
//! ---
//! Change log:
//!   - 2024/03/15: File created.

mod spinlock;
mod intrlock;
mod interrupt_safe_cell;

pub use spinlock::{Spinlock, SpinlockGuard};
pub use intrlock::{Intrlock, IntrlockGuard};
pub use interrupt_safe_cell::{InterruptSafeCell};

