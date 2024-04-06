mod init_binary;

#[repr(C)]
struct AlignedBytes<Align, Bytes: ?Sized> {
    _align: [Align; 0],
    bytes: Bytes,
}

pub static INIT_BINARY: &'static [u8] = &init_binary::ALIGNED_INIT_BINARY.bytes;

