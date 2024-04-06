use alloc::collections::BTreeMap;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::info;
use crate::config::{KERNEL_SPACE_BASE, PROCESS_MMAP_BASE, PROCESS_USER_STACK_BASE};
use crate::device::timer::handler;
use crate::memory::{Addr, PAGE_SIZE, PageTable, PhyAddr, PhyPage, PTEFlags, VirtAddr, VirtPageId};
use crate::utils::error::{EmptyResult, Result};

pub struct ProcessMemory {
    page_table: PageTable,
    // TODO: to make CoW, PhyPage could be shared. So Arc may be needed.
    maps: BTreeMap<VirtPageId, (PhyPage, PTEFlags)>,
    // program binary end. brk should never goes below this
    pub prog_end: VirtAddr,
    // brk is not page aligned. Aligned value is real_brk.
    pub min_brk: VirtAddr,
    pub brk: VirtAddr,
    // stack_base/stack_top is always aligned
    pub stack_base: VirtAddr,
    // stack_top is last valid byte in next stack page
    pub stack_top: VirtAddr,
    // mmap_base/mmap_btm is always aligned
    pub mmap_base: VirtAddr,
    /*
            |   kernel   |
            | ---------  |
            | stack base |
            | .........  |
            | stack top  |
            | ---------- |
            |  reserved  |
            | ---------- |
            |  mmap_base |
            |  ........  |
            | ---------- |
            |  reserved  |
            | ---------- |
            |   brk      |
            | ---------- |
            |   prog_end |
            | ---------- |
            |    0x0     |
     */
}

impl ProcessMemory {
    pub fn new() -> Self {
        let mut page_table = PageTable::new();
        // Set kernel huge table entry
        page_table.map_big(
            VirtAddr::from(KERNEL_SPACE_BASE), PhyAddr::from(KERNEL_SPACE_BASE),
            PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
        );
        Self {
            page_table,
            maps: BTreeMap::new(),
            prog_end: VirtAddr::from(0),
            min_brk: VirtAddr::from(0),
            brk: VirtAddr::from(0),
            stack_base: VirtAddr::from(PROCESS_USER_STACK_BASE),
            stack_top: VirtAddr::from(PROCESS_USER_STACK_BASE),
            mmap_base: VirtAddr::from(PROCESS_MMAP_BASE),
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

    pub fn unmap(&mut self, vpn: VirtPageId) -> EmptyResult {
        if let Some(_) = self.maps.remove(&vpn) {
            self.page_table.unmap(vpn.into());
            Ok(())
        } else {
            Err("page is not mapped.".into())
        }
    }

    pub fn is_mapped(&self, vpn: &VirtPageId) -> bool {
        self.maps.contains_key(vpn)
    }

    pub fn set_brk(&mut self, new_brk: VirtAddr) -> usize {
        // brk is last not valid bytes
        let mut real_brk = self.brk.to_offset(-1)/* last valid byte */.round_up();

        if new_brk < self.min_brk {
            // failure return old brk
            // do nothing
        } else {
            let mut offset = new_brk.addr as isize - self.brk.addr as isize;
            if offset == 0 {
                // do nothing
            } else if offset > 0 {
                // FIXME: could overlap with mmap
                while new_brk > real_brk {
                    let page = PhyPage::alloc();
                    self.map(VirtPageId::from(real_brk.to_offset(1isize)), page, PTEFlags::U | PTEFlags::W | PTEFlags::R);
                    real_brk = real_brk.to_offset(PAGE_SIZE as isize);
                    offset -= PAGE_SIZE as isize;
                }
            } else {
                todo!();
            }
            self.brk = new_brk;
        }
        self.brk.addr
    }

    pub fn increase_user_stack(&mut self) {
        let page = PhyPage::alloc();
        let new_stack_vpn = VirtPageId::from(self.stack_top) - 1;
        self.map(new_stack_vpn, page, PTEFlags::U | PTEFlags::W | PTEFlags::R);
        self.stack_top = new_stack_vpn.into();
    }

    pub fn get_mapped_last_page(&self) -> VirtPageId {
        let first_stack_vpn = VirtPageId::from(self.stack_top);
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

        for (vpn, (page, flags)) in &other.maps {
            let child_page = PhyPage::alloc();
            child_page.copy_u8(0, PhyAddr::from(page.id).get_u8(PAGE_SIZE));
            self.map(vpn.clone(), child_page, flags.clone());
        }
    }

    fn check_collapse(&self, start_vpn: VirtPageId, pages: usize, is_increasing: bool) -> bool {
        let range = if is_increasing {
            start_vpn.id..(start_vpn.id + pages)
        } else {
            (start_vpn.id - pages + 1)..(start_vpn.id + 1)
        };
        for vpn in range {
            let vpn = VirtPageId::from(vpn);
            if self.maps.contains_key(&vpn) {
                return true;
            }
        }
        false
    }

    pub fn mmap(&mut self, addr: Option<VirtAddr>, pages: usize, flags: PTEFlags) -> Result<VirtAddr> {
        if let Some(first_vpn) = addr.map(|addr| VirtPageId::from(addr)) {
            for vpn in first_vpn.id..first_vpn.id + pages {
                let vpn = VirtPageId::from(vpn);
                if self.maps.contains_key(&vpn) {
                    // drop old
                    self.unmap(vpn).unwrap();
                }
                let page = PhyPage::alloc();
                self.map(vpn, page, flags);
            }
            Ok(first_vpn.into())
        } else {
            // addr is not fixed, we alloc this.
            let first_page = {
                let mmap_base_vpn = VirtPageId::from(self.mmap_base) - 1;
                if (!self.maps.contains_key(&mmap_base_vpn)) &&
                    (!self.check_collapse(mmap_base_vpn, pages, false)) {
                    Some(mmap_base_vpn - pages + 1)
                } else {
                    let brk_page = VirtPageId::from(self.brk.to_offset(-1).round_up());
                    self.maps.keys()
                        .filter_map(|mapped_vpn| {
                            if mapped_vpn >= &VirtPageId::from(self.mmap_base) || mapped_vpn <= &brk_page {
                                None
                            } else {
                                let next = mapped_vpn.clone() - 1;
                                if self.maps.contains_key(&next) {
                                    None
                                } else {
                                    Some(next)
                                }
                            }
                        })
                        .find_map(|first_not_mapped_vpn| {
                            if !self.check_collapse(first_not_mapped_vpn, pages, false) {
                                Some(first_not_mapped_vpn - pages + 1)
                            } else {
                                None
                            }
                        })
                }
            };


            if let Some(first_not_mapped_vpn) = first_page {
                for vpn in first_not_mapped_vpn.id..first_not_mapped_vpn.id + pages {
                    let vpn = VirtPageId::from(vpn);
                    let page = PhyPage::alloc();
                    self.map(vpn, page, flags);
                }
                Ok(first_not_mapped_vpn.into())
            } else {
                // no mem
                Err("no enough memory for mmap".into())
            }
        }
    }

    pub fn reset(&mut self) {
        let mut page_table = PageTable::new();
        // Set kernel huge table entry
        page_table.map_big(
            VirtAddr::from(KERNEL_SPACE_BASE), PhyAddr::from(KERNEL_SPACE_BASE),
            PTEFlags::R | PTEFlags::W | PTEFlags::X | PTEFlags::G,
        );
        self.maps.clear();

        self.prog_end = VirtAddr::from(0);
        self.brk = VirtAddr::from(0);
        self.stack_base = VirtAddr::from(PROCESS_USER_STACK_BASE);
        self.stack_top = VirtAddr::from(PROCESS_USER_STACK_BASE);
        self.mmap_base = VirtAddr::from(PROCESS_MMAP_BASE);

        self.page_table = page_table;
    }

    pub fn alloc_stack_if_possible(&mut self, vaddr: VirtAddr) -> bool {
        // if is stack overflow, then try to allocate new stack
        // if user-prog requires too large stack size and new access is beyond next un-allocated page
        // we do not consider it as a stack overflow
        let addr_page = VirtPageId::from(vaddr);
        if addr_page == VirtPageId::from(self.stack_top.to_offset(-1))
            && !self.is_mapped(&addr_page) {
            // is new unallocated stack
            self.increase_user_stack();
            true
        } else {
            false
        }
    }

    pub fn translate_with_stack_alloc(&mut self, vaddr: VirtAddr) -> Option<PhyAddr> {
        if let Some(paddr) = vaddr.into_pa(&self.page_table) {
            return Some(paddr);
        }

        if self.alloc_stack_if_possible(vaddr) {
            Some(vaddr.into_pa(&self.page_table).unwrap())
        } else {
            None
        }
    }
}