use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use bitflags::{bitflags, Flags};
use lazy_static::lazy_static;
use log::info;
use riscv::asm::sfence_vma_all;
use riscv::register::{satp, sstatus};
use crate::core::Spinlock;
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
        // FIXME: 这里的alloc没有清零，放到实体机会bug
        let page = Arc::new(PhyPage::alloc());
        PhyAddr::from(page.id).get_slice_mut::<usize>(PAGE_SIZE / size_of::<usize>()).iter_mut().for_each(|cell| *cell = 0);
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

    pub fn map(&mut self, va: VirtAddr, pa: PhyAddr, flags: PTEFlags) {
        let pte = self.find_pte_create(VirtPageId::from(va)).unwrap();
        assert!(!pte.valid(), "{} is already mapped to {}.", va, PhyAddr::from(pte.page_id()));
        *pte = PageTableEntry::new(PhyPageId::from(pa), flags | PTEFlags::V);
    }

    pub fn unmap(&mut self, va: VirtAddr) {
        let pte = self.find_pte(VirtPageId::from(va)).unwrap();
        assert!(pte.valid(), "{} is not mapped.", va);
        *pte = PageTableEntry::empty();
    }

    pub fn map_big(&mut self, va: VirtAddr, pa: PhyAddr, flags: PTEFlags) {
        let idx = va.addr >> 30;
        let pte = &mut PhyAddr::from(self.entries.id).get_slice_mut::<PageTableEntry>(512)[idx];
        *pte = PageTableEntry::new(PhyPageId::from(pa.addr >> 12), flags | PTEFlags::V);
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

    pub fn unmap_many(&mut self, va:VirtAddr, size: usize) {
        // va must continuous
        for pg in 0..size / PAGE_SIZE {
            self.unmap(va);
        }
    }

    pub fn to_satp(&self) -> usize {
        (self.entries.id.id | 8usize << 60)
    }

    pub fn translate(&self, va: VirtAddr) -> Option<PhyAddr> {
        if let Some(pte) = self.find_pte(VirtPageId::from(va)) {
            let offset = va.addr - va.round_down().addr;
            Some(PhyAddr::from(PhyPageId::from(pte.page_id())).to_offset(offset as isize))
        } else {
            None
        }
    }
}

lazy_static! {
    static ref KERNEL_PAGE_TABLE: Spinlock<PageTable> = Spinlock::new(PageTable::new());
}

pub fn init() {
    info!("In position mapping kernel.");
    let mut kernel_pt = KERNEL_PAGE_TABLE.lock();
    info!("Kernel page table entry: {}", PhyAddr::from(kernel_pt.entries.id));
    kernel_pt.map_big(
        VirtAddr::from(0x80000000), PhyAddr::from(0x80000000),
        PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
    );
    let v = kernel_pt.to_satp();
    satp::write(v);
    sfence_vma_all();
    info!("Paging init complete.");
}

pub fn get_kernel_page_table() -> &'static Spinlock<PageTable> {
    &KERNEL_PAGE_TABLE
}