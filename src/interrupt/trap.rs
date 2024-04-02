//! # Trap
//!
//! Trap handler
//! ---
//! Change log:
//!   - 2024/03/18: File created.

use riscv::register::stvec::TrapMode;
use riscv::register::{sie, sstatus, stvec, time, scause, stval, sepc, satp};
use core::arch::global_asm;
use log::warn;
use log::{error, info, trace};
use riscv::register::scause::{Exception, Scause, Trap};
use riscv::register::sstatus::{SPP, Sstatus};
use crate::cpu::CPU;
use crate::interrupt::interrupt_handler;
use crate::memory::PAGE_SIZE;
use crate::syscall::{Syscall, syscall_handler};

global_asm!(include_str!("trap.S"));

#[repr(C)]
pub struct TrapContext {
    pub reg: [usize; 32],
    // start from 32*8(sp)...
    // Page table PPN for both kernel and user
    pub satp: usize,
    // 出现异常的时候指向触发中断的指令地址
    pub sepc: usize,
    // 状态寄存器
    pub sstatus: usize,
    pub kernel_sp: usize,   // 内核栈sp
}

macro_rules! generate_reg_name_const {
    ($($reg_name:ident),*) => {
        $(
            pub const $reg_name: usize = ${index()};
        )*
    };
}

impl TrapContext {
    generate_reg_name_const!(zero,ra,sp,gp,tp,t0,t1,t2,s0,s1,a0,a1,a2,a3,a4,a5,a6,a7,s2,s3,s4,s5,s6,s7,s8,s9,s10,s11,t3,t4,t5,t6);

    pub fn new() -> Self {
        Self {
            reg: [0; 32],
            satp: 0,
            sepc: 0,
            sstatus: 0,
            kernel_sp: 0,
        }
    }

    pub fn copy_from(&mut self, other: &Self) {
        self.sepc = other.sepc;
        self.reg.copy_from_slice(&other.reg);
    }
}

pub fn set_interrupt_to_kernel() {
    extern "C" {
        fn trap_save_s();
    }
    unsafe {
        stvec::write(trap_save_s as usize, TrapMode::Direct);
    }
}

pub fn set_interrupt_to_user() {
    extern "C" {
        fn trap_save_u();
    }
    let trap_save_u = trap_save_u as usize;
    unsafe {
        stvec::write(trap_save_u, TrapMode::Direct);
    }
}

pub fn enable_trap() {
    unsafe { sstatus::set_sie() }
}

pub fn disable_trap() {
    unsafe { sstatus::clear_sie() }
}

fn exception_handler(trap_context: &TrapContext, exp: scause::Exception, sstatus: sstatus::Sstatus, sepc: usize, stval: usize, from_user: bool) -> Option<usize> {
    // TODO: handle page fault
    match exp {
        Exception::Breakpoint => {
            warn!("Breakpoint triggered.");
            Some(2) // ebreak length
        }
        _ => {
            error!("Exception {:?} from {}: spec: {:#x}, stval: {:#x}", exp,
                    if let SPP::User = sstatus.spp() { "user" } else { "kernel" },
                    sepc, stval);

            if from_user && let Some(proc) = CPU::get_current().unwrap().get_process() {
                error!("Happened on PID {}", proc.pid.pid());
            }

            /*
            unsafe {
                extern "C" {
                    fn boot_sp();
                }
                let stop = if let Some(proc) = CPU::get_current().unwrap().get_process() {
                    proc.data.lock().kernel_stack[0].id.id * PAGE_SIZE
                } else {
                    boot_sp as usize
                };
                let mut fp = trap_context.reg[TrapContext::s0];
                error!("======== RISCV Backtrace ========");
                for i in 0..10 {
                    if fp == stop || fp == 0 {
                        break;
                    }
                    error!("#{}:ra={:#x}", i, *((fp - 8) as *const usize));
                    fp = *((fp - 16) as *const usize);
                }
                error!("=================================");
            }
            */

            let _ = sbi::system_reset::system_reset(
                sbi::system_reset::ResetType::Shutdown,
                sbi::system_reset::ResetReason::SystemFailure,
            );
            // 万一呢？
            loop {}
            None
        }
    }
}

/* Trap handlers */
/*
    发生在S模式下的中断会自动继续运行，不需要手动call sret。
    发生在U模式下的中断不会自动继续运行，需要根据情况call trap_ret_u
 */

#[no_mangle]
fn user_trap_handler(trap_context: &mut TrapContext) {
    set_interrupt_to_kernel();
    let scause = scause::read();
    let stval = stval::read();
    let sepc = sepc::read();
    let sstatus = sstatus::read();
    let satp = satp::read();

    assert_eq!(sstatus.spp(), SPP::User, "User trap not from user!");
    match scause.cause() {
        Trap::Interrupt(int) => {
            interrupt_handler(int);
        }
        Trap::Exception(exp) => {
            match exp {
                Exception::UserEnvCall => {
                    let args = [
                        trap_context.reg[10],
                        trap_context.reg[11],
                        trap_context.reg[12],
                        trap_context.reg[13],
                        trap_context.reg[14],
                        trap_context.reg[15]
                    ]; // make slice sized
                    let id = trap_context.reg[17];
                    trap_context.sepc += 4;
                    let ret = syscall_handler(Syscall::from(id), &args);
                    trap_context.reg[TrapContext::a0] = ret;
                }
                _ => {
                    if let Some(skip_bytes) = exception_handler(trap_context, exp, sstatus, sepc, stval, true) {
                        trap_context.sepc += skip_bytes;
                    }
                }
            }
        }
    }
    user_trap_returner();
}

pub fn user_trap_returner() {
    extern "C" {
        fn trap_ret_u(trap_context: &TrapContext);
    }
    disable_trap();
    let proc = CPU::get_current().unwrap().get_process().unwrap();
    let trap_context = {
        let mut data = proc.data.lock();
        data.get_trap_context()
    };
    drop(proc);
    set_interrupt_to_user();
    unsafe {
        sstatus::set_spp(SPP::User);
        sstatus::set_spie();
        sepc::write(trap_context.sepc);
        trap_ret_u(trap_context);
    }
}

#[no_mangle]
fn kernel_trap_handler(trap_context: &mut TrapContext) {
    let scause = scause::read();
    let stval = stval::read();
    let sepc = sepc::read();
    let sstatus = sstatus::read();
    let satp = satp::read();

    if sstatus.sie() {
        unsafe { sstatus::clear_sie(); }
    }
    assert_eq!(sstatus.spp(), SPP::Supervisor, "Kernel trap not from kernel!");

    match scause.cause() {
        Trap::Interrupt(int) => {
            trace!("Interrupt {:?} triggered.", int);
            interrupt_handler(int);
        }
        Trap::Exception(exp) => {
            if let Some(skip_bytes) = exception_handler(trap_context, exp, sstatus, sepc, stval, false) {
                trap_context.sepc += skip_bytes;
            }
        }
    }

    assert_ne!(trap_context.sstatus & 1 << 8, 0, "Kernel trap leave without spp=1!");
}
