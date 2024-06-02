use core::mem::size_of;
use lazy_static::lazy_static;
use log::info;
use riscv::register::mhartid;
use sbi::base::probe_extension;
use sbi::hart_mask;
use crate::config::HARDWARE_BASE_ADDR;
use crate::cpu::CPU;
use crate::memory::{Addr, flush_page_table, get_kernel_page_table, PAGE_SIZE, PhyAddr, PTEFlags, VirtAddr};
use crate::println;
use crate::startup::get_boot_fdt;

const PLIC_INT_PRIO_OFFSET: usize = 0x0;
const PLIC_INT_ENBITS_OFFSET: usize = 0x2000;
// for each context
const PLIC_INT_ENBITS_CONTEXT_SIZE: usize = 0x80;
const PLIC_INT_ENBITS_SIZE: usize = (size_of::<u32>());

const PLIC_MISC_CONTEXT_OFFSET: usize = 0x200000;
const PLIC_MISC_CONTEXT_SIZE: usize = 0x1000;
const PLIC_MISC_PRIO_THRESHOLD: usize = 0x0;
const PLIC_MISC_CLAIM_COMPLETE: usize = 0x4;

/*

#define PLIC_INT_ENABLE_ADDR_FOR(context, irq)                                 \
    (((uint32_t *)((uintptr_t)(PLIC_VA + PLIC_INTENBITS_OFFSET +               \
                               (context)*PLIC_INTENBITS_CONTEXT_SIZE))) +      \
     ((irq) / PLIC_INTENBITS_SIZE))

#define PLIC_ENABLE_INT_FOR(context, irq)                                      \
    MEM_IO_WRITE(                                                              \
        uint32_t, PLIC_INT_ENABLE_ADDR_FOR(context, irq),                      \
        MEM_IO_READ(uint32_t, PLIC_INT_ENABLE_ADDR_FOR(context, irq)) |        \
            (1 << ((irq) % PLIC_INTENBITS_SIZE)))

#define PLIC_MISC_ADDR_FOR(context)                                            \
    (PLIC_VA + PLIC_MISC_CONTEXT_OFFSET + (context)*PLIC_MISC_CONTEXT_SIZE)

 */

enum PLICPriority {
    Machine = 0,
    Supervisor = 1,
    MAX = 2,
}

struct PLIC(VirtAddr);

impl PLIC {
    pub fn load_from_fdt() -> Self {
        let fdt = get_boot_fdt();
        let (start, size) = fdt.find_compatible(&["riscv,plic0"]).map(|node| {
            let (start, size) = node.reg().unwrap().find_map(|reg| Some((reg.starting_address, reg.size.unwrap()))).unwrap();
            if size < PAGE_SIZE || size % PAGE_SIZE != 0 || start as usize % PAGE_SIZE != 0 {
                panic!("PLIC with unaligned size/addr is not supported.")
            }
            (start, size)
        }).expect("PLIC not Found");

        let vaddr = VirtAddr::from(start as usize + HARDWARE_BASE_ADDR);
        let paddr = PhyAddr::from(start as usize);
        get_kernel_page_table().lock().map_many(vaddr, paddr, size, PTEFlags::W | PTEFlags::R);
        info!("PLIC @ {} mapped to {}", paddr, vaddr);
        flush_page_table(None);
        Self(vaddr)
    }

    fn write(&self, offset: usize, data: u32) {
        *(self.0.to_offset(offset as _).get_ref_mut()) = data;
    }

    fn read(&self, offset: usize) -> u32 {
        *(self.0.to_offset(offset as _).get_ref())
    }

    fn get_context_id(hartid: usize, priority: PLICPriority) -> usize {
        hartid * (PLICPriority::MAX as usize) + priority as usize
    }

    fn get_misc_offset_for_hart(hartid: usize, priority: PLICPriority) -> usize {
        PLIC_MISC_CONTEXT_OFFSET + PLIC_MISC_CONTEXT_SIZE * Self::get_context_id(hartid, priority)
    }

    fn get_int_enable_offset_for_hart(hartid: usize, priority: PLICPriority) -> usize {
        PLIC_INT_ENBITS_OFFSET + PLIC_INT_ENBITS_CONTEXT_SIZE * Self::get_context_id(hartid, priority)
    }

    fn get_int_enable_offset_for_hart_and_irq(hartid: usize, priority: PLICPriority, irq: usize) -> usize {
        Self::get_int_enable_offset_for_hart(hartid, priority) + irq / PLIC_INT_ENBITS_SIZE
    }

    pub fn write_misc(&self, hartid: usize, priority: PLICPriority, offset: usize, data: u32) {
        self.write(Self::get_misc_offset_for_hart(hartid, priority) + offset, data)
    }

    pub fn read_misc(&self, hartid: usize, priority: PLICPriority, offset: usize) -> u32 {
        self.read(Self::get_misc_offset_for_hart(hartid, priority) + offset)
    }

    pub fn set_irq_priority(&self, irq: usize, priority: PLICPriority) {
        self.write(PLIC_INT_PRIO_OFFSET + irq * size_of::<u32>(), priority as u32);
    }

    pub fn enable_irq(&self, hartid: usize, priority: PLICPriority, irq: usize) {
        let offset = Self::get_int_enable_offset_for_hart_and_irq(hartid, priority, irq);
        let old = self.read(offset);
        self.write(offset, old | 1 << (irq % PLIC_INT_ENBITS_SIZE));
    }

    pub fn disable_irq(&self, hartid: usize, priority: PLICPriority, irq: usize) {
        let offset = Self::get_int_enable_offset_for_hart_and_irq(hartid, priority, irq);
        let old = self.read(offset);
        self.write(offset, old & (!(1u32 << (irq % PLIC_INT_ENBITS_SIZE))));
    }
}

lazy_static! {
    static ref PLIC_HANDLER: PLIC = PLIC::load_from_fdt();
}

pub fn init() {
    info!("Initialize PLIC");
    for hartid in 0..CPU::get_count() {
        PLIC_HANDLER.write_misc(hartid, PLICPriority::Supervisor, PLIC_MISC_PRIO_THRESHOLD, 0);
    }
}

pub fn claim() -> usize {
    let hartid = CPU::get_current_id();
    PLIC_HANDLER.read_misc(hartid, PLICPriority::Supervisor, PLIC_MISC_CLAIM_COMPLETE) as usize
}

pub fn complete(irq: usize) {
    let hartid = CPU::get_current_id();
    PLIC_HANDLER.write_misc(hartid, PLICPriority::Supervisor, PLIC_MISC_CLAIM_COMPLETE, irq as u32);
}

pub fn enable_irq(irq: usize) {
    PLIC_HANDLER.set_irq_priority(irq, PLICPriority::Supervisor);
    for hartid in 0..CPU::get_count() {
        PLIC_HANDLER.enable_irq(hartid, PLICPriority::Supervisor, irq);
    }
}

pub fn disable_irq(irq: usize) {
    for hartid in 0..CPU::get_count() {
        PLIC_HANDLER.disable_irq(hartid, PLICPriority::Supervisor, irq);
    }
    PLIC_HANDLER.set_irq_priority(irq, PLICPriority::Machine);
}