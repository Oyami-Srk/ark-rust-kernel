use log::info;
use riscv::register::{sie, scause::Interrupt, time};

mod trap;

pub use trap::{enable_trap, disable_trap, TrapContext, set_interrupt_to_kernel, user_trap_returner};
use crate::cpu::CPU;
use crate::process;

pub fn init() {
    set_interrupt_to_kernel();
    unsafe {
        sie::set_sext();
        sie::set_ssoft();
        sie::set_stimer();
    }
    sbi::timer::set_timer(time::read64() + 1000000).expect("Set timer failed"); // 一秒钟100 ticks
    enable_trap();
}

pub fn interrupt_handler(scause: Interrupt) {
    match scause {
        Interrupt::SupervisorTimer => {
            sbi::timer::set_timer(time::read64() + 1000000).expect("Set timer failed"); // 一秒钟1 ticks
            if CPU::get_current().unwrap().get_process().is_some() {
                process::do_yield();
            }
        },
        _ => ()
    }
}