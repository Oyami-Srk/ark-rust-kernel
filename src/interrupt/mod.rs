use log::info;
use riscv::register::{sie, scause::Interrupt, time};

mod trap;

pub use trap::{enable_trap, disable_trap, TrapContext, set_interrupt_to_kernel, user_trap_returner};
use crate::cpu::CPU;
use crate::{device, process};

pub fn init() {
    set_interrupt_to_kernel();
    unsafe {
        sie::set_sext();
        sie::set_ssoft();
        sie::set_stimer();
    }
    enable_trap();
}

pub fn interrupt_handler(scause: Interrupt) {
    match scause {
        Interrupt::SupervisorTimer => device::timer::handler(),
        _ => ()
    }
}