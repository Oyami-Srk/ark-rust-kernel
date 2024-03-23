# 切换到另一个内核执行任务
.altmacro

.macro SAVE_S_REG n
    sd s\n, (\n+2)*8(a0)
.endm
.macro LOAD_S_REG n
    ld s\n, (\n+2)*8(a1)
.endm

.section .text,"ax"
.global context_switch
.align 4

context_switch:
    # a0: *mut TaskContext ( To be switched out )
    # a1: *const TaskContext ( To be switched in )

    sd ra, 0*8(a0)
    sd sp, 1*8(a0)
    .set n, 0
    .rept 12
        SAVE_S_REG %n
        .set n, n + 1
    .endr

    .set n, 0
    .rept 12
        LOAD_S_REG %n
        .set n, n + 1
    .endr
    ld sp, 1*8(a1)
    ld ra, 0*8(a1)

    # 返回的是TaskContext中交换来的新ra
    ret