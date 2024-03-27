use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::info;
use crate::memory::{Addr, PAGE_SIZE, PageTable, PhyAddr, PhyPage, PTEFlags, VirtAddr, VirtPageId};

pub struct ProcessMemory {
    page_table: PageTable,
    // TODO: to make CoW, PhyPage could be shared. So Arc may be needed.
    maps: BTreeMap<VirtPageId, (PhyPage, PTEFlags)>,
    // program binary end. brk should never goes below this
    pub prog_end: VirtAddr,
    // brk is not page aligned. Aligned value is real_brk.
    pub brk: VirtAddr,
    // stack_base/stack_top is always aligned
    pub stack_base: VirtAddr,
    pub stack_top: VirtAddr,
    /*
            |   kernel   |
            | ---------  |
            | stack top  |
            | .........  |
            | stack base |
     */
}

impl ProcessMemory {
    pub fn new() -> Self {
        let mut page_table = PageTable::new();
        // Set kernel huge table entry
        page_table.map_big(
            VirtAddr::from(0x8000_0000), PhyAddr::from(0x8000_0000),
            PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
        );
        Self {
            page_table,
            maps: BTreeMap::new(),
            prog_end: VirtAddr::from(0),
            brk: VirtAddr::from(0),
            stack_base: VirtAddr::from(0x8000_0000),
            stack_top: VirtAddr::from(0x8000_0000),
        }
    }

    pub fn get_satp(&self) -> usize {
        self.page_table.to_satp()
    }

    pub fn get_pagetable(&self) -> &PageTable {
        &self.page_table
    }

    pub fn map(&mut self, vpn: VirtPageId, page: PhyPage, flags: PTEFlags) {
        // info!("[satp {:x}] Map {} to {}",self.page_table.to_satp() ,VirtAddr::from(vpn), PhyAddr::from(page.id));
        // take page
        self.page_table.map(vpn.clone().into(), page.id.into(), flags.clone());
        self.maps.insert(vpn, (page, flags));
    }

    pub fn set_brk(&mut self, offset: isize) -> usize {
        let mut real_brk = self.brk.round_up();

        if offset == 0 {} else if offset < 0 {
            todo!()
        } else {
            let new_brk = self.brk.to_offset(offset);
            let mut offset = offset;
            while new_brk >= real_brk {
                let page = PhyPage::alloc();
                self.map(VirtPageId::from(real_brk), page, PTEFlags::U | PTEFlags::W | PTEFlags::R);
                real_brk = real_brk.to_offset(PAGE_SIZE as isize);
                offset -= PAGE_SIZE as isize;
            }
            self.brk = new_brk;
        }
        self.brk.addr
    }

    pub fn increase_user_stack(&mut self) {
        let page = PhyPage::alloc();
        let stack_base = VirtPageId::from(self.stack_base) - 1usize;
        self.map(stack_base, page, PTEFlags::U | PTEFlags::W | PTEFlags::R);
        self.stack_base = stack_base.into();
    }

    pub fn get_mapped_last_page(&self) -> VirtPageId {
        let first_stack_vpn = VirtPageId::from(self.stack_base);
        let end = self.maps.iter().filter_map(|(vpn, _)| {
            if *vpn >= first_stack_vpn {
                None
            } else {
                Some(vpn)
            }
        }).max_by_key(|p| p.id);
        end.cloned().unwrap_or(VirtPageId::from(0))
    }

    pub fn copy_from(&mut self, other: &Self, copy_stack: bool) {
        self.stack_top = other.stack_top;
        self.stack_base = other.stack_base;
        self.brk = other.brk;
        self.prog_end = other.prog_end;

        other.maps.iter().for_each(|(vpn, (page, flags))| {
            let child_page = PhyPage::alloc();
            child_page.copy_u8(0, PhyAddr::from(page.id).get_u8(PAGE_SIZE));
            self.map(vpn.clone(), child_page, flags.clone());
        });
    }
}