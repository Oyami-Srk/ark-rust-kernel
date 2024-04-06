use bitflags::{bitflags, Flags};
use crate::cpu::CPU;
use crate::filesystem::SeekPosition;
use crate::memory::{Addr, PAGE_SIZE, PTEFlags, VirtAddr, VirtPageId};
use crate::syscall::error::{SyscallError, SyscallResult};

pub fn brk(addr: usize) -> SyscallResult {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    Ok(proc_data.memory.set_brk(addr.into()))
}


bitflags! {
    pub struct ProtFlags: usize {
        const PROT_NONE = 0x0;
        const PROT_READ = 0x1;
        const PROT_WRITE = 0x2;
        const PROT_EXEC = 0x4;
    }
}

bitflags! {
    pub struct MapFlags: usize {
        const MAP_SHARED = 0x1;
        const MAP_PRIVATE = 0x2;
        const MAP_FIXED = 0x10;
        const MAP_ANONYMOUS = 0x20;
    }
}

pub fn mmap(addr: VirtAddr, len: usize, prot: usize, flags: usize, fd: usize, offset: usize) -> SyscallResult {
    let len = VirtAddr::from(len).round_up().addr;
    if len == 0 {
        return Ok(-1isize as usize);
    }
    let offset = offset as isize;
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();

    let prot = ProtFlags::from_bits(prot).unwrap();
    let flags = MapFlags::from_bits(flags).unwrap();

    let mut pte_flags = PTEFlags::U;
    if prot.contains(ProtFlags::PROT_READ) {
        pte_flags |= PTEFlags::R;
    }
    if prot.contains(ProtFlags::PROT_WRITE) {
        pte_flags |= PTEFlags::W;
    }
    if prot.contains(ProtFlags::PROT_EXEC) {
        pte_flags |= PTEFlags::X;
    }

    let pages_count = (len + addr.addr % PAGE_SIZE) / PAGE_SIZE;
    let virt_addr = if flags.contains(MapFlags::MAP_FIXED) {
        // 如果是FIXED，重叠区域会被释放然后重新映射
        Some(proc_data.memory.mmap(Some(addr), pages_count, pte_flags).unwrap())
    } else {
        proc_data.memory.mmap(None, pages_count, pte_flags).ok()
    };

    if let Some(start_addr) = virt_addr {
        if addr.addr % PAGE_SIZE != 0 || offset != 0 {
            todo!("Unaligned mmap read file is not impl yet.");
        }
        if !flags.contains(MapFlags::MAP_ANONYMOUS) {
            let vpn = VirtPageId::from(start_addr);
            if let Some(Some(file)) = proc_data.files.get(fd) {
                file.seek(offset, SeekPosition::Set).expect("Seek failed");
                for pg in vpn.id..vpn.id + pages_count {
                    let this_vpn = VirtPageId::from(pg);
                    file.read(
                        VirtAddr::from(this_vpn).into_pa(proc_data.memory.get_pagetable()).unwrap()
                            .get_slice_mut::<u8>(PAGE_SIZE)
                    ).expect("Read failed");
                }
            } else {
                // Failed to open file
                for pg in vpn.id..vpn.id + pages_count {
                    proc_data.memory.unmap(VirtPageId::from(pg)).expect("Failed to unmap mmap failed pages");
                }
                return Err(SyscallError::EIO);
            }
        }
        Ok(start_addr.get_addr())
    } else {
        Err(SyscallError::ENOMEM)
    }
}

pub fn munmap(addr: VirtAddr, len: usize) -> SyscallResult {
    let pages_count = (len + addr.addr % PAGE_SIZE) / PAGE_SIZE;
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let mut proc_data = proc.data.lock();
    let first_vpn = VirtPageId::from(addr);
    for pg in first_vpn.id .. first_vpn.id + pages_count {
        proc_data.memory.unmap(VirtPageId::from(pg)).expect("Failed to unmap pages in munmap.");
    }

    Ok(0)
}