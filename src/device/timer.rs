use lazy_static::lazy_static;
use riscv::register::scause::set;
use riscv::register::time;
use crate::config::{CLOCK_FREQ, TICKS_PER_SECOND};
use crate::cpu::CPU;
use crate::process;
use crate::process::Condvar;

lazy_static! {
    static ref TIMER_CONDVAR: Condvar = Condvar::new();
}

#[inline]
fn set_next_trigger() {
    sbi::timer::set_timer(time::read64() + (CLOCK_FREQ / TICKS_PER_SECOND) as u64).expect("Set timer failed");
}

pub fn init() {
    set_next_trigger();
}

pub fn handler() {
    set_next_trigger();
    TIMER_CONDVAR.wakeup();
    if CPU::get_current().unwrap().get_process().is_some() {
        process::do_yield();
    }
}

pub fn sleep_on_timer() {
    TIMER_CONDVAR.wait();
    process::do_yield();
}