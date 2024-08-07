use log::warn;
use riscv::asm::ebreak;
use riscv::register::time;
use crate::config::{CLOCK_FREQ, TICKS_PER_SECOND};
use crate::cpu::CPU;
use crate::device::timer;
use crate::memory::{Addr, VirtAddr};
use crate::syscall::error::SyscallResult;

pub fn sleep_ticks(ticks: usize) -> SyscallResult {
    let current_ticks = time::read64() as usize / (CLOCK_FREQ / TICKS_PER_SECOND);
    while (time::read64() as usize / (CLOCK_FREQ / TICKS_PER_SECOND)) - current_ticks < ticks {
        timer::sleep_on_timer();
    }
    Ok(time::read64() as usize / (CLOCK_FREQ / TICKS_PER_SECOND))
}

pub fn breakpoint(id: usize, data: VirtAddr, optional_length: usize) -> SyscallResult {
    let proc = CPU::get_current_process().unwrap();
    let proc_data = proc.data.lock();
    if id == 0 {
        // data is c string
        let cstr = data.into_pa(proc_data.memory.get_pagetable()).unwrap().get_cstr();
        warn!("Breakpoint with string: {}", cstr);
    }

    unsafe { ebreak(); };
    Ok(0)
}