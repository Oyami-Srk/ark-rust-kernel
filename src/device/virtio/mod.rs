mod block;

use core::mem::size_of;
use core::ptr::NonNull;
use log::info;
use crate::startup::get_boot_fdt;
use virtio_drivers::{BufferDirection, Hal, PhysAddr};
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, Transport};
use crate::config::HARDWARE_BASE_ADDR;
use crate::memory::{PhyPage, VirtAddr, alloc_page_without_trace, dealloc_page_without_trace, PhyAddr, PhyPageId, PAGE_SIZE, get_kernel_page_table, Addr, PTEFlags, flush_page_table};
use crate::utils::error::EmptyResult;

pub struct VirtioHal;

unsafe impl Hal for VirtioHal {
    fn dma_alloc(pages: usize, direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let page_id_start = unsafe { alloc_page_without_trace(pages) };
        let paddr = PhyAddr::from(PhyPageId::from(page_id_start));
        // zeros buffer
        for id in page_id_start..page_id_start + pages {
            PhyAddr::from(PhyPageId::from(id))
                .get_slice_mut(PAGE_SIZE / size_of::<usize>())
                .iter_mut().for_each(|addr| { *addr = 0 });
        }
        let vaddr = NonNull::new(paddr.get_ref_mut()).unwrap();
        (paddr.addr, vaddr)
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, vaddr: NonNull<u8>, pages: usize) -> i32 {
        let page_id_start = PhyPageId::from(PhyAddr::from(paddr)).id;
        unsafe { dealloc_page_without_trace(page_id_start, pages) }
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, size: usize) -> NonNull<u8> {
        NonNull::new(paddr as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, direction: BufferDirection) -> PhysAddr {
        let vaddr = buffer.as_ptr() as *mut u8 as usize;
        let vaddr = VirtAddr::from(vaddr);
        if vaddr.addr < HARDWARE_BASE_ADDR {
            vaddr.addr
        } else {
            vaddr.into_pa(&crate::memory::get_kernel_page_table().lock()).unwrap().addr
        }
    }

    unsafe fn unshare(paddr: PhysAddr, buffer: NonNull<[u8]>, direction: BufferDirection) {
        // Nothing to do here. Since host already access to all memory.
    }
}

pub fn init() {
    info!("VirtIO initialize for MMIO.");
    let fdt = get_boot_fdt();
    fdt.find_all_nodes("/soc/virtio_mmio").for_each(|node| {
        let (start, size) = node.reg().unwrap().find_map(|reg| Some((reg.starting_address, reg.size.unwrap()))).unwrap();
        let vaddr = VirtAddr::from(start as usize + HARDWARE_BASE_ADDR);
        let paddr = PhyAddr::from(start as usize);
        if size < PAGE_SIZE || size % PAGE_SIZE != 0 || start as usize % PAGE_SIZE != 0 {
            panic!("VirtIO with unaligned size/addr is not supported.")
        }
        get_kernel_page_table().lock().map_many(vaddr, paddr, size, PTEFlags::W | PTEFlags::R);
        flush_page_table(None);
        let header = NonNull::new(vaddr.get_addr() as *mut VirtIOHeader).unwrap();
        let transport = unsafe { MmioTransport::new(header) };
        if let Ok(transport) = transport {
            let type_ = transport.device_type();
            info!("Found {}. Type: {:?}, start: 0x{:x}, size: 0x{:x}", node.name, type_, start as usize, size);
            match type_ {
                DeviceType::Block => {
                    block::init(
                        VirtIOBlk::new(transport).unwrap(),
                        node.interrupts().unwrap().find_map(|i| Some(i)).unwrap(),
                    )
                }
                _ => {}
            }
        }
    });
}