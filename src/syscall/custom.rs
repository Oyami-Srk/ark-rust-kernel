use riscv::register::time;
use crate::config::{CLOCK_FREQ, TICKS_PER_SECOND};
use crate::device::timer;

pub fn sleep_ticks(ticks: usize) -> usize {
    let current_ticks = time::read() / (CLOCK_FREQ / TICKS_PER_SECOND);
    while (time::read() / (CLOCK_FREQ / TICKS_PER_SECOND)) - current_ticks < ticks {
        timer::sleep_on_timer();
    }
    time::read() / (CLOCK_FREQ / TICKS_PER_SECOND)
}