use alloc::boxed::Box;
use alloc::{format, vec};
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::ops::Deref;
use fatfs::{DefaultTimeProvider, Dir, FileSystem, IoBase, LossyOemCpConverter, Read, Seek, SeekFrom, Write};
use log::info;
use crate::core::Spinlock;
use crate::filesystem::{DirEntry, DirEntryType, File, FileModes, FileOpenFlags, Filesystem, Inode, register_filesystem, SeekPosition};
use crate::utils::error::{EmptyResult, KernelError, Result as KernelResult};

struct FatFSDeviceWrapper {
    file: Arc<dyn File>,
}

impl FatFSDeviceWrapper {
    pub fn new(file: Arc<dyn File>) -> Self {
        Self {
            file
        }
    }
}

impl IoBase for FatFSDeviceWrapper {
    type Error = ();
}

impl Read for FatFSDeviceWrapper {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let read_bytes = self.file.read(buf).map_err(|_| ())?;
        Ok(read_bytes)
    }
}

impl Write for FatFSDeviceWrapper {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.file.write(buf).map_err(|_| ())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // TODO: implement flush
        Ok(())
    }
}

impl Seek for FatFSDeviceWrapper {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        let (offset, whence) = match pos {
            SeekFrom::Start(offset) => (offset as isize, SeekPosition::Set),
            SeekFrom::End(offset) => (offset as isize, SeekPosition::End),
            SeekFrom::Current(offset) => (offset as isize, SeekPosition::Cur)
        };
        self.file.seek(offset, whence).map_err(|_| ()).map(|i| i as u64)
    }
}

enum FatFSInodeType {
    Dir,
    File,
}

struct FatFSInode {
    path: String,
    type_: FatFSInodeType,
    mountpoint: Option<Arc<FileSystem<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>>>,
    fs: &'static FileSystem<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>,
}

impl Drop for FatFSInode {
    fn drop(&mut self) {}
}

impl Inode for FatFSInode {
    fn lookup(&self, name: &str) -> Option<DirEntry> {
        let search_through_iter = |name: &str, path, dir: Dir<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>, fs| -> Option<DirEntry> {
            if let Some(dir) = dir.iter().find(
                |v| if let Ok(v) = v { v.file_name() == name } else { false }) {
                let dir = dir.unwrap();
                let inode = FatFSInode {
                    path,
                    type_: if dir.is_dir() { FatFSInodeType::Dir } else { FatFSInodeType::File },
                    mountpoint: None,
                    fs,
                };
                let dentry = DirEntry {
                    parent: None,
                    name: name.to_string(),
                    inode: Some(Arc::new(inode)),
                    type_: if dir.is_dir() { DirEntryType::Dir } else { DirEntryType::File },
                    children: Spinlock::new(Vec::new()),
                };
                Some(dentry)
            } else {
                None
            }
        };

        if let Some(fs) = &self.mountpoint {
            let root_dir = fs.root_dir();
            let fs = Arc::as_ptr(fs);
            let fs = unsafe { fs.as_ref::<'static>().unwrap() };
            search_through_iter(name, format!("/{}", name), root_dir, fs)
        } else {
            match &self.type_ {
                FatFSInodeType::Dir => {
                    let fs = self.fs;
                    let dir = fs.root_dir().open_dir(self.path.as_str()).unwrap();
                    search_through_iter(name, format!("{}/{}", self.path, name), dir, fs)
                }
                FatFSInodeType::File => { None }
            }
        }
    }

    fn link(&self, inode: Arc<dyn Inode>, name: &str) -> EmptyResult {
        todo!()
    }

    fn unlink(&self, name: &str) -> EmptyResult {
        todo!()
    }

    fn mkdir(&self, name: &str) -> crate::utils::error::Result<Arc<dyn Inode>> {
        todo!()
    }

    fn rmdir(&self, name: &str) -> EmptyResult {
        todo!()
    }

    fn read_dir(&self) -> crate::utils::error::Result<Vec<DirEntry>> {
        todo!()
    }

    fn open(&self, dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> crate::utils::error::Result<Arc<dyn File>> {
        let fs = self.fs;
        let dir = fs.root_dir();
        let file = dir.open_file(self.path.as_str()).unwrap();
        Ok(Arc::new(FatFSFile {
            dentry,
            file: Spinlock::new(file),
        }))
    }

    fn get_dentry_type(&self) -> DirEntryType {
        match &self.type_ {
            FatFSInodeType::Dir => DirEntryType::Dir,
            FatFSInodeType::File => DirEntryType::File,
        }
    }
}

struct FatFSFile<'a> {
    dentry: Arc<DirEntry>,
    file: Spinlock<fatfs::File<'a, FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>>,
}

impl<'a> File for FatFSFile<'a> {
    fn seek(&self, offset: isize, whence: SeekPosition) -> KernelResult<usize> {
        let mut file = self.file.lock();
        file.seek(match whence {
            SeekPosition::Set => SeekFrom::Start(offset as u64),
            SeekPosition::Cur => SeekFrom::Current(offset as i64),
            SeekPosition::End => SeekFrom::End(offset as i64)
        }).map(|v| v as usize).map_err(|e| "seek failed for fatfs.".into())
    }

    fn read(&self, buf: &mut [u8]) -> KernelResult<usize> {
        let mut file = self.file.lock();
        let mut read_bytes = 0;
        loop {
            let this_read_bytes = file.read(&mut buf[read_bytes..]).unwrap();
            if this_read_bytes == 0 { break }
            read_bytes += this_read_bytes;
        }
        Ok(read_bytes)
    }

    fn write(&self, buf: &[u8]) -> KernelResult<usize> {
        todo!()
    }

    fn close(&self) -> EmptyResult {
        // We will drop everything
        Ok(())
    }

    fn get_dentry(&self) -> Arc<DirEntry> {
        self.dentry.clone()
    }
}

struct FatFS {}

impl Filesystem for FatFS {
    fn new() -> Self {
        Self {}
    }

    fn mount(&self, device: Option<Arc<dyn File>>, mount_point: Arc<DirEntry>) -> KernelResult<Arc<dyn Inode>> {
        let device = device.ok_or("Must provided device file for fatfs")?;
        let fs = fatfs::FileSystem::new(FatFSDeviceWrapper::new(device), fatfs::FsOptions::new()).unwrap();
        fs.root_dir().iter().for_each(|i| {
            info!("{}", i.unwrap().file_name());
        });
        let fs = Arc::new(fs);
        let fs_p = Arc::as_ptr(&fs);
        let fs_ref = unsafe { fs_p.as_ref::<'static>().unwrap() };
        Ok(Arc::new(FatFSInode {
            path: "/".to_string(),
            type_: FatFSInodeType::Dir,
            mountpoint: Some(fs),
            fs: fs_ref,
        }))
    }
}

pub fn init() {
    register_filesystem("fatfs", Box::new(FatFS::new()));
}