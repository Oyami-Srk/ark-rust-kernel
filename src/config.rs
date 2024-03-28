pub const PROCESS_KERNEL_STACK_SIZE: usize = 32; // in pages. 4K * 32 = 128KB
pub const CLOCK_FREQ: usize = 10000000; // Got from https://github.com/qemu/qemu/blob/master/include/hw/intc/riscv_aclint.h#L78
pub const TICKS_PER_SECOND: usize = 10;
pub const TIMER_INTERVAL: usize = CLOCK_FREQ / TICKS_PER_SECOND;