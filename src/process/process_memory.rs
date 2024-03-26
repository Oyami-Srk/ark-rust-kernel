use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::memory::{Addr, PAGE_SIZE, PageTable, PhyAddr, PhyPage, PTEFlags, VirtAddr, VirtPageId};

pub struct ProcessMemory {
    page_table: PageTable,
    // TODO: to make CoW, PhyPage could be shared. So Arc may be needed.
    maps: BTreeMap<VirtPageId, PhyPage>,
    // program binary end. brk should never goes below this
    pub prog_end: VirtAddr,
    // brk is not page aligned. Aligned value is real_brk.
    pub brk: VirtAddr,
    // stack_base is always aligned
    pub stack_base: VirtAddr,
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
        }
    }

    pub fn get_satp(&self) -> usize {
        self.page_table.to_satp()
    }

    pub fn get_pagetable(&self) -> &PageTable {
        &self.page_table
    }

    pub fn map(&mut self, vpn: VirtPageId, page: PhyPage, flags: PTEFlags) {
        // take page
        self.page_table.map(vpn.clone().into(), page.id.into(), flags);
        self.maps.insert(vpn, page);
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

    pub fn get_mapped_last_page(&self) ->  VirtPageId {
        let first_stack_vpn = VirtPageId::from(self.stack_base);
        let end = self.maps.iter().filter_map( |(vpn,_)| {
            if *vpn >= first_stack_vpn {
                None
            } else {
                Some(vpn)
            }
        }).max_by_key(|p| p.id);
        end.cloned().unwrap_or(VirtPageId::from(0))
    }
}