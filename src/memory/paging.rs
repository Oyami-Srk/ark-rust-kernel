use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use bitflags::{bitflags, Flags};
use lazy_static::lazy_static;
use log::{debug, info, trace};
use riscv::asm;
use riscv::register::{satp, sstatus};
use crate::config::HARDWARE_BASE_ADDR;
use crate::core::Spinlock;
use crate::cpu::{CPUID, VendorId};
use crate::interrupt::enable_trap;
use crate::memory::address::{VirtAddr, VirtPageId, Addr};
use crate::memory::PAGE_SIZE;
use super::{PhyPageId, PhyPage, PhyAddr};

bitflags! {
    #[derive(Copy, Clone)]
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    pub fn new(page_id: PhyPageId, flags: PTEFlags) -> Self {
        Self(page_id.id << 10 | flags.bits() as usize)
    }

    pub fn empty() -> Self {
        Self(0)
    }

    pub fn page_id(&self) -> PhyPageId {
        (self.0 >> 10 & ((1usize << 44) - 1)).into()
    }

    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.0 as u8).unwrap()
    }

    pub fn valid(&self) -> bool {
        self.flags().contains(PTEFlags::V)
    }

    pub fn readable(&self) -> bool {
        self.flags().contains(PTEFlags::R)
    }

    pub fn writable(&self) -> bool {
        self.flags().contains(PTEFlags::W)
    }

    pub fn executable(&self) -> bool {
        self.flags().contains(PTEFlags::X)
    }
}

pub struct PageTable {
    entries: Arc<PhyPage>,
    pages: Vec<Arc<PhyPage>>,
}

impl PageTable {
    pub fn new() -> Self {
        let page = Arc::new(PhyPage::alloc());
        Self {
            entries: page.clone(),
            pages: vec![page],
        }
    }

    pub fn find_pte(&self, vpn: VirtPageId) -> Option<&mut PageTableEntry> {
        let idxes = vpn.get_pte_indexes();

        let mut ppn = self.entries.id;
        let mut result: Option<&mut PageTableEntry> = None;

        for (i, idx) in idxes.iter().enumerate() {
            let pte = &mut PhyAddr::from(ppn).get_slice_mut::<PageTableEntry>(512)[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.valid() {
                return None;
            }
            ppn = PhyPageId::from(pte.page_id().id);
        }
        result
    }

    pub fn find_pte_create(&mut self, vpn: VirtPageId) -> Option<&mut PageTableEntry> {
        let idxes = vpn.get_pte_indexes();

        let mut ppn = self.entries.id;
        for (i, idx) in idxes.iter().enumerate() {
            let pte =
                &mut PhyAddr::from(ppn).get_slice_mut::<PageTableEntry>(512)[*idx];
            if i == 2 {
                return Some(pte);
            }
            if !pte.valid() {
                let page = PhyPage::alloc();
                *pte = PageTableEntry::new(page.id, PTEFlags::V);
                self.pages.push(Arc::new(page));
            }
            ppn = pte.page_id();
        }
        None
    }

    fn prepare_flags(pa: PhyAddr, va: VirtAddr, flags: PTEFlags) -> PTEFlags {
        let mut flags = flags;
        if flags.contains(PTEFlags::R) {
            flags |= PTEFlags::A;
        }
        if flags.contains(PTEFlags::W) {
            flags |= PTEFlags::D;
        }
        flags |= PTEFlags::V;
        flags
    }

    fn set_pte(pte: &mut PageTableEntry, pa: PhyAddr, va: VirtAddr, flags: PTEFlags) {
        assert!(!pte.valid(), "{} is already mapped to {}.", va, PhyAddr::from(pte.page_id()));
        /* From privileged spec:
            The A and D bits are never cleared by the implementation. If the supervisor software does
            not rely on accessed and/or dirty bits, e.g. if it does not swap memory pages to secondary storage
            or if the pages are being used to map I/O space, it should always set them to 1 in the PTE to
            improve performance.
         */
        *pte = PageTableEntry::new(PhyPageId::from(pa), Self::prepare_flags(pa, va, flags));
        // Special treatment for C906
        if CPUID.get_vendor() == VendorId::THead {
            let mut extend_bits: usize = 0;
            if va >= VirtAddr::from(HARDWARE_BASE_ADDR) {
                // is device memory
                extend_bits |= (1usize << 63); // Strong order
            } else {
                extend_bits |= (1usize << 62); // Cacheable
                extend_bits |= (1usize << 61); // Buffer-able
            }
            let pte_bits = pte as *mut PageTableEntry as *mut usize;
            unsafe { *pte_bits |= extend_bits };
        }
    }

    pub fn map(&mut self, va: VirtAddr, pa: PhyAddr, flags: PTEFlags) {
        let pte = self.find_pte_create(VirtPageId::from(va)).unwrap();
        Self::set_pte(pte, pa, va, flags)
    }

    pub fn unmap(&mut self, va: VirtAddr) {
        let pte = self.find_pte(VirtPageId::from(va)).unwrap();
        assert!(pte.valid(), "{} is not mapped.", va);
        *pte = PageTableEntry::empty();
    }

    pub fn map_big(&mut self, va: VirtAddr, pa: PhyAddr, flags: PTEFlags) {
        let idx = va.addr >> 30;
        let pte = &mut PhyAddr::from(self.entries.id).get_slice_mut::<PageTableEntry>(512)[idx];
        Self::set_pte(pte, pa, va, flags)
    }

    pub fn map_many(&mut self, va: VirtAddr, pa: PhyAddr, size: usize, flags: PTEFlags) {
        // pa must continuous
        for pg in 0..size / PAGE_SIZE {
            self.map(
                va.to_offset((PAGE_SIZE * pg) as isize),
                pa.to_offset((PAGE_SIZE * pg) as isize),
                flags);
        }
    }

    pub fn unmap_many(&mut self, va: VirtAddr, pages: usize) {
        // va must continuous
        for pg in 0..pages {
            self.unmap(VirtAddr::from(VirtPageId::from(va) + pg));
        }
    }

    pub fn to_satp(&self) -> usize {
        (self.entries.id.id | 8usize << 60)
    }

    pub fn translate(&self, va: VirtAddr) -> Option<PhyAddr> {
        if let Some(pte) = self.find_pte(VirtPageId::from(va)) {
            if pte.valid() {
                let offset = va.addr - va.round_down().addr;
                Some(PhyAddr::from(PhyPageId::from(pte.page_id())).to_offset(offset as isize))
            } else {
                None
            }
        } else {
            None
        }
    }
}

lazy_static! {
    static ref KERNEL_PAGE_TABLE: Spinlock<PageTable> = Spinlock::new(PageTable::new());
}

pub fn flush_page_table(va: Option<VirtAddr>) {
    if let Some(va) = va {
        asm::fence_i();
        unsafe { asm::sfence_vma(0, va.get_addr()) };
        asm::fence_i();
    } else {
        asm::fence_i();
        asm::sfence_vma_all();
        asm::fence_i();
    }
}

pub fn init() {
    trace!("In position mapping kernel.");
    let mut kernel_pt = KERNEL_PAGE_TABLE.lock();
    debug!("Kernel page table entry: {}", PhyAddr::from(kernel_pt.entries.id));
    kernel_pt.map_big(
        VirtAddr::from(0x80000000), PhyAddr::from(0x80000000),
        PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
    );
    let v = kernel_pt.to_satp();
    satp::write(v);
    flush_page_table(None);
    info!("Paging initialization complete.");
}

pub fn get_kernel_page_table() -> &'static Spinlock<PageTable> {
    &KERNEL_PAGE_TABLE
}