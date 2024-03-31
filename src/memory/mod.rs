//! # Memory
//!
//! Memory management
//! ---
//! Change log:
//!   - 2024/03/15: File created.

mod page_allocator;
mod address;
mod paging;

use log::info;
pub use address::{PhyAddr, PhyPageId, VirtAddr, VirtPageId, Addr};
pub use page_allocator::{PhyPage, alloc_page_without_trace, dealloc_page_without_trace};
pub use paging::{PageTable, PTEFlags, get_kernel_page_table};

pub const PAGE_SIZE: usize = 4096;

pub struct MemoryInfo {
    pub start: PhyAddr,
    pub end: PhyAddr,
    pub usable_start: PhyAddr,
    pub usable_end: PhyAddr,
}

pub fn init() {
    page_allocator::init();
    paging::init();
}
