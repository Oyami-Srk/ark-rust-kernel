use core::fmt::Write;
use crate::cpu::CPU;
use crate::memory::VirtAddr;
use crate::{print, println};

pub fn write(args: &[usize; 6]) -> usize {
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let proc_data = proc.data.lock();
    let page_table = &proc_data.page_table;

    let fd = args[0];
    let buf = VirtAddr::from(args[1]);
    let len = args[2];
    let buf = page_table.translate(buf).unwrap();
    let str = unsafe { core::slice::from_raw_parts(buf.get_ref_mut(), len) };
    let str = core::str::from_utf8(str).unwrap();

    print!("{}", str);

    0
}