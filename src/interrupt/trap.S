.altmacro

.macro SAVE_REG n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_REG n
    ld x\n, \n*8(sp)
.endm

.section .text,"ax"
.global trap_save_s
.global trap_save_u
.global trap_ret_u

.align 8
trap_save_s:
    # 保存
    addi sp, sp, -36*8

    sd x1, 1*8(sp)
    .set n, 3
    .rept 29
        SAVE_REG %n
        .set n, n+1
    .endr

    csrr t0, sepc
    sd t0, 33*8(sp)
    csrr t0, sstatus
    sd t0, 34*8(sp)

    # 处理, 第一个参数是TrapContext的地址
    mv a0, sp
    call kernel_trap_handler

    # 恢复
    ld t0, 34*8(sp)
    csrw sstatus, t0
    ld t0, 33*8(sp)
    csrw sepc, t0

    sd x1, 1*8(sp)
    .set n, 3
    .rept 29
        LOAD_REG %n
        .set n, n+1
    .endr

    addi sp, sp, 36*8
    sret

.align 8
trap_save_u:
    # 交换sp和sscratch，现在sp中存放的是TrapContext的指针
    csrrw sp, sscratch, sp
    sd x1, 1*8(sp)
    # 跳过保存sp
    # 保存 x3~x31
    .set n, 3
    .rept 29
        SAVE_REG %n
        .set n, n+1
    .endr

    csrr t0, sepc
    sd t0, 33*8(sp)
    csrr t0, sstatus
    sd t0, 34*8(sp)
    # sscratch里是原本的sp
    csrr t0, sscratch
    sd t0, 2*8(sp)

    # 不使用蹦床设计，我们直接把内核映射到所有用户页上。

    # t0 = satp (user)
    csrr t0, satp
    # t1 = TrapContext.user_satp
    ld t1, 32*8(sp)
    # satp = t1
    csrw satp, t1
    # TrapContext.user_satp = t0
    sd t0, 32*8(sp)

    # 恢复内核栈
    addi a0, sp, 0
    ld sp, 35*8(sp)

    # 跳转到Rust中的handler
    j user_trap_handler

trap_ret_u:
    # a0: TrapContext指针
    # switch page table

    # t1 = satp (kernel)
    csrr t1, satp
    # t0 = TrapContext.user_satp
    ld t0, 32*8(a0)
    # satp = t0
    csrw satp, t0
    sfence.vma
    # TrapContext.user_satp = t1
    sd t1, 32*8(a0)

    # sscratch 持有TrapContext指针
    csrw sscratch, a0
    mv sp, a0

    ld t0, 33*8(sp)
    csrw sepc, t0
    ld t0, 34*8(sp)
    csrw sstatus, t0

    ld x1, 1*8(sp)
    # 跳过恢复sp
    .set n, 3
    .rept 29
        LOAD_REG %n
        .set n, n+1
    .endr

    # 恢复sp
    ld sp, 2*8(sp)
    sret
