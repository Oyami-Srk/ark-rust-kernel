use alloc::sync::Weak;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use crate::core::Spinlock;
use crate::cpu::CPU;
use crate::process::{Process, ProcessStatus};

pub struct Condvar {
    pub data: Spinlock<CondvarData>,
}

pub struct CondvarData {
    pub waiting_list: Vec<Weak<Process>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            data: Spinlock::new(CondvarData {
                waiting_list: vec![],
            })
        }
    }

    pub fn wakeup(&self) {
        let mut data = self.data.lock();
        while let Some(proc) = data.waiting_list.pop() {
            if let Some(proc) = proc.upgrade() {
                proc.data.lock().status = ProcessStatus::Ready;
            }
        }
    }

    pub fn wait(&self) {
        // TODO: 这里只是等待了，但是没有对关键区持有一个锁，这样会导致如果有竞态在当前进程加入等待列表之前就调用了wakeup，会导致唤醒丢失的问题。
        let proc = CPU::get_current().unwrap().get_process().unwrap();
        proc.data.lock().status = ProcessStatus::Suspend;
        self.data.lock().waiting_list.push(Arc::downgrade(&proc));
    }
}
