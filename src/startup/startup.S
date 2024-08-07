# Unlike x86, we do not need setup GDT or goto higher address
# Because of SBI. but I kinda feel uncool for that.
.section .text.startup,"ax"
.global _start

_start:
    # Make FIP/ATF Happy.
    j _real_start
    .balign 4
    .word 0x33334c42
    .word 0xdeadbeea
    .word 0xdeadbeeb
    .quad 0x80200000
    .word 0xdeadbeec
    .balign 4
    j _real_start
    .balign 4

    # SBI jump to kernel with a0 = mhartid, a1 = fdt_addr
    _real_start:

    # Mask all interrupts
    csrw sie, 0

    # Clear bss
    # Only boot core (mhart=0) clear the bss
    bnez a0, .clear_out
    # bss is aligned to 8 bytes
    la a2, __bss_start
    la a3, __bss_end
    .clear_start:
        bge a2, a3, .clear_out
        sw zero, 0(a2)
        addi a2, a2,4
        j .clear_start
    .clear_out:

    # a0 = mhartid, set by SBI
    add t0, a0, 1
    # t0 = t0 << 17 = mhartid * 2 ^ 17;
    slli t0, t0, 17
    la sp, boot_stack
    # Each CPU core got 64KB Boot stack
    add sp, sp, t0

    # Direct jump into kernel, platform entrance
    call startup

    # loop for unreached code
    .loop:
        j .loop

.section .bss.boot_stack
.global boot_stack
boot_stack:
    # 512KB kernel boot stack For ecah core (assume there would be maximum 4 cores)
    .space 1024 * 512 * 4
.global boot_sp
boot_sp:
