//! # Page Allocator
//!
//! Page allocator
//! ---
//! Change log:
//!   - 2024/03/17: File created.

use alloc::format;
use alloc::vec::Vec;
use core::mem::size_of;
use buddy_system_allocator::{LockedFrameAllocator, LockedHeap};
use lazy_static::lazy_static;
use log::{info, trace};
use crate::core::Spinlock;
use crate::memory::{Addr, PAGE_SIZE};
use crate::startup;
use super::address::{PhyAddr, PhyPageId};

/* Heap allocator */

const KERNEL_HEAP_SIZE: usize = 1024 * 1024 * 2; // 2 MB early kernel heap size

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

static mut KERNEL_HEAP_SPACE: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];

pub fn init() {
    // TODO: Use our own page allocator to do CoW and reference
    unsafe {
        HEAP_ALLOCATOR.lock().init(KERNEL_HEAP_SPACE.as_ptr().addr(), KERNEL_HEAP_SIZE);
    }
    unsafe {
        let start_page_id = PhyPageId::from(startup::get_boot_memory_info().usable_start) + 1;
        let end_page_id = PhyPageId::from(startup::get_boot_memory_info().usable_end);
        info!("Add {} to {} to PageAllocator, totally {} pages.", start_page_id, end_page_id, end_page_id.id - start_page_id.id);
        PAGE_ALLOCATOR.lock().add_frame(start_page_id.id, end_page_id.id);
    }
}

/* Min-heap page allocator */
struct PageAreas {

}

struct MinHeapPageAllocator {

}
/*
impl MinHeapPageAllocator {
    pub fn new() -> Self {
    }

    /// Add a range of frame number [start, end) to the allocator
    pub fn add_frame(&mut self, start: usize, end: usize) {
        assert!(start <= end);
    }

    /// Add a range of frames to the allocator.
    pub fn insert(&mut self, range: Range<usize>) {
    }

    /// Allocate a range of frames from the allocator, returning the first frame of the allocated
    /// range.
    pub fn alloc(&mut self, count: usize) -> Option<usize> {
    }

    /// Deallocate a range of frames [frame, frame+count) from the frame allocator.
    ///
    /// The range should be exactly the same when it was allocated, as in heap allocator
    pub fn dealloc(&mut self, start_frame: usize, count: usize) {
    }
}
*/
/* Page allocator */
lazy_static! {
    static ref PAGE_ALLOCATOR: LockedFrameAllocator<32> = LockedFrameAllocator::new();
}

pub struct PhyPage {
    pub id: PhyPageId,
}

impl PhyPage {
    pub fn new(id: PhyPageId) -> Self {
        // info!("Tracing a phy page at {:x}", id.id * PAGE_SIZE);
        // TODO: clear
        /*
        let pg_addr = PhyAddr::from(id);
        let usizes = pg_addr.get_slice_mut::<usize>(PAGE_SIZE / size_of::<usize>());
        usizes.iter_mut().for_each(|cell| {
            unsafe { (cell as *mut usize).write_volatile(0); }
        });
        */
        Self {
            id
        }
    }

    pub fn alloc() -> Self {
        Self::new(PAGE_ALLOCATOR.lock().alloc(1).expect("Allocate 1 page failed.").into())
    }

    pub fn alloc_many(count: usize) -> Vec<Self> {
        let start_id = PAGE_ALLOCATOR.lock().alloc(count).expect(format!("Allocate {} page failed", count).as_str());
        (start_id..start_id + count).map(|id| Self::new(id.into())).collect()
    }

    pub fn get_ref<T>(&self) -> &'static T {
        PhyAddr::from(self.id).get_ref()
    }

    pub fn get_ref_mut<T>(&self) -> &'static mut T {
        PhyAddr::from(self.id).get_ref_mut()
    }

    pub fn copy_u8(&self, offset: usize, data: &[u8]) {
        // assume no overlapping
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(),
                                           PhyAddr::from(self.id).to_offset(offset as isize).get_ref_mut(),
                                           data.len());
        }
    }
}

impl Drop for PhyPage {
    fn drop(&mut self) {
        // info!("Dropping a phy page at {:x}", self.id.id * PAGE_SIZE);
        PAGE_ALLOCATOR.lock().dealloc(self.id.id, 1);
    }
}

