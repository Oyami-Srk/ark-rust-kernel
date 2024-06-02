use alloc::format;
use alloc::sync::{Arc, Weak};
use bitflags::*;
use alloc::vec::Vec;
use crate::core::Spinlock;
use crate::filesystem::{DirEntry, DirEntryType, File, FileModes, FileOpenFlags, Inode, InodeStat, SeekPosition};
use crate::process::{Condvar, do_yield};
use crate::utils::error::{EmptyResult,Result};

const PIPE_SIZE: usize = 512;

struct PipeBuffer {
    data: [u8; PIPE_SIZE],
    n_read: usize,
    n_write: usize,
    pub read_open: bool,
    pub write_open: bool,
    pub wait_read: Condvar,
    pub wait_write: Condvar,
}

impl PipeBuffer {
    pub fn new() -> Self {
        Self {
            data: [0; PIPE_SIZE],
            n_read: 0,
            n_write: 0,
            read_open: false,
            write_open: false,
            wait_read: Condvar::new(),
            wait_write: Condvar::new(),
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.n_read == self.n_write {
            if self.write_open {
                Some(0)
            } else {
                None
            }
        } else {
            let mut i = 0;
            while !(self.n_read == self.n_write || i == buf.len()) {
                buf[i] = self.data[self.n_read % PIPE_SIZE];
                i += 1;
                self.n_read += 1;
            }
            if i != 0 {
                self.wait_write.wakeup();
            }
            Some(i)
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Option<usize> {
        let mut wrote_bytes = 0;
        for c in buf {
            if (self.n_write + 1) == (self.n_read + PIPE_SIZE) % PIPE_SIZE {
                // write enough
                break;
            }
            self.data[self.n_write % PIPE_SIZE] = c.clone();
            self.n_write += 1;
            wrote_bytes += 1;
        }
        if wrote_bytes != 0 {
            self.wait_read.wakeup();
        }
        if self.read_open {
            // Someone is reading
            Some(wrote_bytes)
        } else {
            // No one is reading
            None
        }
    }
}

enum PipeFileType {
    Reader,
    Writer,
}

pub struct PipeFile {
    type_: PipeFileType,
    buffer: Arc<Spinlock<PipeBuffer>>,
}

impl File for PipeFile {
    fn seek(&self, offset: isize, whence: SeekPosition) -> crate::utils::error::Result<usize> {
        Err("Cannot seek pipe.".into())
    }

    fn read(&self, buf: &mut [u8]) -> crate::utils::error::Result<usize> {
        match self.type_ {
            PipeFileType::Reader => {
                loop {
                    let mut buffer = self.buffer.lock();
                    let result = buffer.read(buf);
                    if let Some(read_bytes) = result {
                        if read_bytes == 0 && buffer.write_open {
                            // Read nothing but writer is open
                            buffer.wait_write.wait();
                            drop(buffer);
                            do_yield();
                        } else {
                            return Ok(read_bytes);
                        }
                    } else {
                        return Err("Read from a pipe no one could write.".into());
                    }
                }
            }
            PipeFileType::Writer => { Err("Cannot read from writer.".into()) }
        }
    }

    fn write(&self, buf: &[u8]) -> crate::utils::error::Result<usize> {
        match self.type_ {
            PipeFileType::Writer => {
                let mut total_wrote = 0;
                loop {
                    let mut buffer = self.buffer.lock();
                    let result = buffer.write(&buf[total_wrote..]);
                    if let Some(wrote_bytes) = result {
                        total_wrote += wrote_bytes;
                    } else {
                        return Err("Write to a pipe no one could read.".into());
                    }
                    if total_wrote != buf.len() {
                        // Write is not complete
                        buffer.wait_read.wait();
                        drop(buffer);
                        do_yield();
                    } else {
                        // Write complete
                        break;
                    }
                }
                Ok(total_wrote)
            }
            PipeFileType::Reader => { Err("Cannot write by reader.".into()) }
        }
    }

    fn close(&self) -> EmptyResult {
        let mut buffer = self.buffer.lock();
        match self.type_ {
            PipeFileType::Reader => {
                buffer.read_open = false;
                buffer.wait_write.wakeup();
            }
            PipeFileType::Writer => {
                buffer.write_open = false;
                buffer.wait_read.wakeup();
            }
        }
        Ok(())
    }

    fn get_dentry(&self) -> Result<Arc<DirEntry>> {
        let name =
            format!("pipe-{}", match self.type_ {
                PipeFileType::Reader => { "read" }
                PipeFileType::Writer => { "write" }
            });
        let dummy_inode = PipeDummyInode::new(
            FileModes::FIFO | match self.type_ {
                PipeFileType::Reader => { FileModes::Read }
                PipeFileType::Writer => { FileModes::Write }
            }
        );
        let dummy_dentry = DirEntry::new(None, name, Some(Arc::new(dummy_inode)), DirEntryType::File);
        Ok(Arc::new(dummy_dentry))
    }
}

impl PipeFile {
    pub fn create() -> (Self, Self) {
        let mut buffer = PipeBuffer::new();
        buffer.write_open = true;
        buffer.read_open = true;
        let buffer = Arc::new(Spinlock::new(buffer));
        let reader = Self {
            type_: PipeFileType::Reader,
            buffer: buffer.clone(),
        };
        let writer = Self {
            type_: PipeFileType::Writer,
            buffer,
        };
        (reader, writer)
    }
}

struct PipeDummyInode {
    mode: usize,
}

impl Drop for PipeDummyInode { fn drop(&mut self) {} }

impl Inode for PipeDummyInode {
    fn lookup(&self, name: &str, this_dentry: Weak<DirEntry>) -> Option<DirEntry> {
        None
    }

    fn link(&self, inode: Arc<dyn Inode>, name: &str) -> EmptyResult {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn unlink(&self, name: &str) -> EmptyResult {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn mkdir(&self, name: &str) -> crate::utils::error::Result<Arc<dyn Inode>> {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn rmdir(&self, name: &str) -> EmptyResult {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn read_dir(&self, this_dentry: Weak<DirEntry>) -> crate::utils::error::Result<Vec<DirEntry>> {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn open(&self, dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> crate::utils::error::Result<Arc<dyn File>> {
        Err("Cannot operate to pipe dummy inode.".into())
    }

    fn get_dentry_type(&self) -> DirEntryType {
        DirEntryType::File
    }

    fn get_stat(&self) -> InodeStat {
        InodeStat {
            ino: 0,
            mode: self.mode,
            nlink: 1,
            size: PIPE_SIZE,
            block_size: 1,
        }
    }
}

impl PipeDummyInode {
    pub fn new(mode: FileModes) -> Self {
        Self {
            mode: mode.bits() as usize
        }
    }
}