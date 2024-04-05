use core::convert::TryFrom;
use c_defines_to_enum::parse_c_defines_to_enum;

parse_c_defines_to_enum!(
    Syscall,
    remove_prefix = "SYS_",
    content = include_str!("c/syscall_id.h")
);
