use lazy_static::lazy_static;
use log::info;
use riscv::register::{sie, scause::Interrupt, time};

mod trap;
pub mod plic;

pub use trap::{enable_trap, disable_trap, TrapContext, set_interrupt_to_kernel, user_trap_returner};
use crate::cpu::CPU;
use crate::{device, process};
use crate::utils::error::EmptyResult;
use crate::core::Spinlock;

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
        Interrupt::SupervisorExternal => {
            let irq = plic::claim();
            if irq != 0 {
                if let Some(func) = get_interrupt_handler(irq) {
                    func();
                }
                plic::complete(irq);
            }
        },
        _ => ()
    }
}

const MAX_INTERRUPT: usize = 64;

lazy_static! {
    static ref INTERRUPT_TABLE: Spinlock<[Option<InterruptHandlerFn>; MAX_INTERRUPT]> = Spinlock::new([None; MAX_INTERRUPT]);
}

type InterruptHandlerFn = fn() -> ();
pub fn register_interrupt_handler(irq: usize, handler: InterruptHandlerFn) -> EmptyResult {
    let mut table = INTERRUPT_TABLE.lock();
    if table[irq].is_some() {
        Err("Already registered.".into())
    } else {
        table[irq] = Some(handler);
        Ok(())
    }
}

pub fn unregister_interrupt_handler(irq: usize, handler: InterruptHandlerFn) -> EmptyResult {
    let mut table = INTERRUPT_TABLE.lock();
    if let Some(f) = table[irq] {
        if f == handler {
            table[irq] = None;
            Ok(())
        } else {
            Err("Not registered handler.".into())
        }
    } else {
        Err("No handler registered.".into())
    }
}

pub fn get_interrupt_handler(irq: usize) -> Option<InterruptHandlerFn> {
    INTERRUPT_TABLE.lock()[irq]
}