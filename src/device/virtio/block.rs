use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::{max, min};
use core::ops::DerefMut;
use bitflags::Flags;
use lazy_static::lazy_static;
use log::info;
use virtio_drivers::device::blk::{BlkReq, BlkResp, RespStatus, SECTOR_SIZE, VirtIOBlk};
use virtio_drivers::transport::mmio::MmioTransport;
use virtio_drivers::transport::Transport;
use crate::device::virtio::VirtioHal;
use crate::filesystem::{DirEntry, DirEntryType, File, FileModes, FileOpenFlags, Inode, SeekPosition, DirFile};
use crate::utils::error::EmptyResult;
use crate::core::Spinlock;
use crate::interrupt::{plic, register_interrupt_handler};
use crate::memory::{Addr, PAGE_SIZE, PhyAddr, PhyPage, VirtAddr};
use crate::{process, utils};
use crate::process::Condvar;

#[derive(Copy, Clone, PartialEq)]
pub enum VirtIOBlockRequestType {
    Write,
    Read,
}

struct VirtIOBlockFile {
    device: Arc<VirtIOBlock>,
    dentry: Arc<DirEntry>,
    cur: Spinlock<usize>,
}

impl File for VirtIOBlockFile {
    fn seek(&self, offset: isize, whence: SeekPosition) -> crate::utils::error::Result<usize> {
        let mut cur = self.cur.lock();
        match whence {
            SeekPosition::Set => { *cur = offset as usize }
            SeekPosition::Cur => { *cur = if offset.is_positive() { *cur + offset.unsigned_abs() } else { *cur - offset.unsigned_abs() } }
            SeekPosition::End => { *cur = if offset.is_positive() { self.device.size } else { self.device.size - offset.unsigned_abs() } }
        }
        if *cur > self.device.size {
            *cur = self.device.size
        }
        Ok(*cur)
    }

    fn read(&self, buf: &mut [u8]) -> crate::utils::error::Result<usize> {
        let mut offset = self.cur.lock();
        let read_bytes = self.device.read(*offset, buf);
        *offset += read_bytes;
        Ok(read_bytes)
    }

    fn write(&self, buf: &[u8]) -> crate::utils::error::Result<usize> {
        todo!()
    }

    fn close(&self) -> EmptyResult {
        todo!()
    }

    fn get_dentry(&self) -> Arc<DirEntry> {
        self.dentry.clone()
    }
}

impl VirtIOBlockFile {
    pub fn new(device: Arc<VirtIOBlock>, dentry: Arc<DirEntry>) -> Self {
        Self {
            device,
            dentry,
            cur: Spinlock::new(0),
        }
    }
}

struct VirtIOBlockInode {
    device: Arc<VirtIOBlock>,
}

impl Inode for VirtIOBlockInode {
    fn lookup(&self, name: &str) -> Option<DirEntry> {
        unimplemented!()
    }

    fn link(&self, inode: Arc<dyn Inode>, name: &str) -> EmptyResult {
        unimplemented!()
    }

    fn unlink(&self, name: &str) -> EmptyResult {
        unimplemented!()
    }

    fn mkdir(&self, name: &str) -> crate::utils::error::Result<Arc<dyn Inode>> {
        unimplemented!()
    }

    fn rmdir(&self, name: &str) -> EmptyResult {
        unimplemented!()
    }

    fn read_dir(&self) -> crate::utils::error::Result<Vec<DirEntry>> {
        unimplemented!()
    }

    fn open(&self, dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> crate::utils::error::Result<Arc<dyn File>> {
        // TODO: respect open flags.
        Ok(Arc::new(VirtIOBlockFile::new(
            self.device.clone(),
            dentry,
        )))
    }

    fn get_dentry_type(&self) -> DirEntryType {
        DirEntryType::File
    }
}

impl Drop for VirtIOBlockInode {
    fn drop(&mut self) {
        todo!()
    }
}

impl VirtIOBlockInode {
    pub fn new(device: Arc<VirtIOBlock>) -> Self {
        Self {
            device,
        }
    }
}

struct VirtIOBlock {
    device: Spinlock<VirtIOBlk<VirtioHal, MmioTransport>>,
    condvars: BTreeMap<u16, (Condvar, Spinlock<Option<(*mut BlkReq, *mut BlkResp, *mut [u8], VirtIOBlockRequestType)>>)>,
    size: usize, // in bytes
}

impl VirtIOBlock {
    pub fn new(device: VirtIOBlk<VirtioHal, MmioTransport>) -> Self {
        let size = device.capacity() as usize * SECTOR_SIZE;
        let max_idx = device.virt_queue_size();
        let mut condvars = BTreeMap::new();
        for i in 0..max_idx {
            condvars.insert(i, (Condvar::new(), Spinlock::new(None)));
        }
        Self {
            device: Spinlock::new(device),
            condvars,
            size,
        }
    }

    pub fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        assert_eq!(buf.len() % SECTOR_SIZE, 0, "Read block only accepts buf aligned with SECTOR_SIZE");
        /*
        let mut resp = BlkResp::default();
        let mut req = BlkReq::default();
        let idx = unsafe { self.device.lock().read_blocks_nb(block_id, &mut req, buf, &mut resp) }
            .expect("Failed to send read request");
        let (condvar, data) = self.condvars.get(&idx).unwrap();
        condvar.wait();
        *data.lock() = Some((&mut req as *mut BlkReq, &mut resp as *mut BlkResp, buf as *mut [u8], VirtIOBlockRequestType::Read));
        process::do_yield();
        assert_eq!(resp.status(), RespStatus::OK, "Failed to read result.");
        */

        self.device.lock().read_blocks(block_id, buf).unwrap();
    }

    pub fn read(&self, offset: usize, buf: &mut [u8]) -> usize {
        assert!(offset <= self.size, "Read go beyond size.");
        let block_id = offset / SECTOR_SIZE;
        let in_block_offset = offset % SECTOR_SIZE;
        let mut must_read_size = in_block_offset + buf.len();
        if must_read_size + block_id * SECTOR_SIZE > self.size {
            must_read_size = self.size - block_id * SECTOR_SIZE;
        }
        let rounded_size = utils::round_up_to(must_read_size, SECTOR_SIZE);
        let pgs = utils::round_up_to(rounded_size, PAGE_SIZE) / PAGE_SIZE;
        let pgs = PhyPage::alloc_many(pgs);
        let kbuf = PhyAddr::from(pgs.first().unwrap().id).get_slice_mut(rounded_size);
        self.read_block(block_id, kbuf);
        let read_size = if buf.len() + in_block_offset + block_id * SECTOR_SIZE > self.size {
            self.size - block_id * SECTOR_SIZE - in_block_offset
        } else {
            buf.len()
        };
        buf.copy_from_slice(&kbuf[in_block_offset..in_block_offset + read_size]);
        read_size
    }

    pub fn handle_irq(&self) {
        // info!("int.");
        let mut device = self.device.lock();
        while let Some(idx) = device.peek_used() {
            if let Some(condvar) = self.condvars.get(&idx) {
                // self.device.lock().complete_read_blocks();
                // condvar.wakeup();
                let (condvar, data) = condvar;
                if let Some((req, resp, buf, type_)) = *data.lock() {
                    unsafe {
                        let req = req.as_mut().unwrap();
                        let resp = resp.as_mut().unwrap();
                        let buf = buf.as_mut().unwrap();
                        match type_ {
                            VirtIOBlockRequestType::Write => {
                                device.complete_write_blocks(idx, req, buf, resp).expect("Failed to complete write");
                            }
                            VirtIOBlockRequestType::Read => {
                                device.complete_read_blocks(idx, req, buf, resp).expect("Failed to complete read");
                            }
                        }
                    }
                    condvar.wakeup();
                } else {
                    panic!("");
                }
            }
        }
    }
}

lazy_static! {
    static ref VIRTIO_BLOCKS: Spinlock<Vec<Arc<VirtIOBlock>>> = Spinlock::new(Vec::new());
}

pub fn init(device: VirtIOBlk<VirtioHal, MmioTransport>, irq: usize) {
    let device = Arc::new(VirtIOBlock::new(device));
    VIRTIO_BLOCKS.lock().push(device.clone());
    let capacity = device.device.lock().capacity() as usize * SECTOR_SIZE;
    info!("Detected {} Bytes virtio-block device.", capacity);

    let dev = DirEntry::from_path("/dev", None).expect("Failed to get /dev on vfs.");
    // TODO: blk0 not hard coded.
    dev.link(Arc::new(VirtIOBlockInode::new(device.clone())), "blk0").expect("Failed to link /dev/blk0 on vfs");

    plic::enable_irq(irq);
    register_interrupt_handler(irq, interrupt_handler).expect("Failed to register interrupt");
}

pub fn interrupt_handler() {
    VIRTIO_BLOCKS.lock().iter().for_each(|device| {
        device.handle_irq();
    });
}