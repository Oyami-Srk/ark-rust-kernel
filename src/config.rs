pub const PROCESS_KERNEL_STACK_SIZE: usize = 256; // in pages. 4K * 128 = 512KB
pub const CLOCK_FREQ: usize = 10000000; // Got from https://github.com/qemu/qemu/blob/master/include/hw/intc/riscv_aclint.h#L78
pub const TICKS_PER_SECOND: usize = 10;
pub const TIMER_INTERVAL: usize = CLOCK_FREQ / TICKS_PER_SECOND;
pub const HARDWARE_BASE_ADDR: usize = 0xC000_0000;

pub const KERNEL_HEAP_SIZE_EARLY: usize = 1024 * 1024 * 1; // 1 MB early kernel heap size
pub const KERNEL_HEAP_SIZE_IN_MEM: usize = 1024 * 1024 * 64; // 64 MB in-mem kernel heap size
