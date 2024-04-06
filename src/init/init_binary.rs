use super::AlignedBytes;

pub(super) static ALIGNED_INIT_BINARY: &'static AlignedBytes<usize, [u8]> = &AlignedBytes {
    _align: [],
    bytes: *include_bytes!("../../../user/target/riscv64gc-unknown-none-elf/debug/init"),
};
