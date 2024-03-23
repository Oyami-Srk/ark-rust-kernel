//! # Page Allocator
//!
//! Page allocator
//! ---
//! Change log:
//!   - 2024/03/17: File created.

use alloc::format;
use alloc::vec::Vec;
use buddy_system_allocator::{LockedFrameAllocator, LockedHeap};
use lazy_static::lazy_static;
use crate::core::Mutex;
use crate::startup;
use super::address::{PhyAddr, PhyPageId};

/* Heap allocator */

const KERNEL_HEAP_SIZE: usize = 1024 * 1024 * 1; // 1 MB kernel heap size

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
        PAGE_ALLOCATOR.lock().add_frame(start_page_id.id, end_page_id.id);
    }
}

/* Page allocator */
lazy_static! {
    static ref PAGE_ALLOCATOR: LockedFrameAllocator<32> = LockedFrameAllocator::new();
}

pub struct PhyPage {
    pub id: PhyPageId,
}

impl PhyPage {
    pub fn new(id: PhyPageId) -> Self {
        Self {
            id
        }
    }

    pub fn alloc() -> Self {
        Self {
            id: PAGE_ALLOCATOR.lock().alloc(1).expect("Allocate 1 page failed.").into(),
        }
    }

    pub fn alloc_many(count: usize) -> Vec<Self> {
        let start_id = PAGE_ALLOCATOR.lock().alloc(count).expect(format!("Allocate {} page failed", count).as_str());
        (start_id..start_id+count).map(|id| Self::new(id.into())).collect()
    }

    pub fn get_ref<T>(&self) -> &'static T {
        PhyAddr::from(self.id).get_ref()
    }

    pub fn get_ref_mut<T>(&self) -> &'static mut T {
        PhyAddr::from(self.id).get_ref_mut()
    }
}

impl Drop for PhyPage {
    fn drop(&mut self) {
        PAGE_ALLOCATOR.lock().dealloc(self.id.id, 1);
    }
}

