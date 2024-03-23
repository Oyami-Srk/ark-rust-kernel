//! # Boot
//!
//! ---
//! Change log:
//!   - 2024/03/19: File created.

use alloc::vec::Vec;
use core::arch::global_asm;
use fdt::Fdt;
use lazy_static::lazy_static;
use log::info;

use crate::memory::MemoryInfo;

// Linker symbols
extern "C" {
    fn _KERN_BASE();
    fn _KERN_END();
}

global_asm!(include_str!("startup.S"));

static mut BOOT_FDT_ADDR: *mut u8 = core::ptr::null_mut();
static mut BOOT_CORE_ID: Option<usize> = None;

lazy_static! {
    static ref BOOT_MEMORY_INFO: MemoryInfo = ((|| {
        let fdt = crate::startup::get_boot_fdt();

        for m in fdt.memory().regions() {
            info!("Memory region detected: {:#x} to {:#x}", m.starting_address.addr(), if m.size.is_none() { 0 } else {
                m.starting_address.addr() + m.size.unwrap()
            } );
        }
        let largest_region = fdt.memory().regions().filter(|r| {
            r.size.is_some()
        }).max_by_key(|r| r.size).expect("Not found any available memory regions");

        let end = (largest_region.starting_address.addr() + largest_region.size.unwrap()).into();

        MemoryInfo {
            start: largest_region.starting_address.addr().into(),
            end,
            usable_start: (_KERN_END as usize).into(),
            usable_end: end,
        }
    })());
}

#[no_mangle]
fn startup(mhardid: u64, fdt_addr: *mut u8) -> ! {
    crate::main(mhardid, if let Some(_) = unsafe { BOOT_CORE_ID } {
        false
    } else {
        unsafe { BOOT_CORE_ID = Some(mhardid as usize) };
        unsafe { BOOT_FDT_ADDR = fdt_addr };
        true
    });
}

pub fn init() {
    let fdt = unsafe {
        fdt::Fdt::from_ptr(unsafe { BOOT_FDT_ADDR }).unwrap()
    };
    let size = fdt.total_size();
    let fdt = unsafe {
        Vec::from_raw_parts(unsafe { BOOT_FDT_ADDR }, size, size)
    }.clone();
    let raw_pointer = fdt.leak();
    info!("Copy FDT from {:#x} to {:#x}.", unsafe {BOOT_FDT_ADDR}.addr(), raw_pointer.as_ptr().addr());
    unsafe { BOOT_FDT_ADDR = raw_pointer.as_mut_ptr() };
}

pub fn get_boot_fdt() -> Fdt<'static> {
    unsafe {
        Fdt::from_ptr(unsafe { BOOT_FDT_ADDR }).unwrap()
    }
}

pub fn get_boot_core_id() -> Option<usize> {
    unsafe { BOOT_CORE_ID }
}

pub fn get_boot_memory_info() -> &'static MemoryInfo {
    &BOOT_MEMORY_INFO
}