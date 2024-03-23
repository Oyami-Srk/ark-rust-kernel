/* link script for qemu-riscv64 */
OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS {
    _KERN_BASE = 0x80200000;
    . = _KERN_BASE;

    .text ALIGN(0x1000): {
        PROVIDE(__start_kernel_code = .);
        *(.text.startup)
        *(.text .stub .text.* .gnu.linkonce.t.*)
        PROVIDE(__end_kernel_code = .);
    }

    .rodata ALIGN(0x1000) : {
        *(.rodata .rodata.* .gnu.linkonce.r.* .init_code)
    }

    .stab ALIGN(0x1000) : { /* Debug information */
        *(.stab .stabstr)
        BYTE(0)
    }

    .data ALIGN(0x1000) : {
        *(.data .data.*)
        *(.sdata .sdata.*)

        . = ALIGN(8);
        /* Driver's data */
        PROVIDE(__start_DevDrivers = .);
        *(DevDrivers*)
        PROVIDE(__stop_DevDrivers = .);

        . = ALIGN(8);
        /* FDT Prober's data */
        PROVIDE(__start_FDTProbers = .);
        *(FDTProbers*)
        PROVIDE(__stop_FDTProbers = .);

        . = ALIGN(8);
        /* Filesystems data */
        PROVIDE(__start_Filesystems = .);
        *(Filesystems*)
        PROVIDE(__stop_Filesystems = .);
    }

    .bss ALIGN(0x1000) : {
        PROVIDE(__bss_start = .);
        *(.bss.boot_stack)
        *(.bss.stack)
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(.ebss .ebss.*)
        . = ALIGN(8);
        PROVIDE(__bss_end = .);
    }

    _KERN_END = .;
}
