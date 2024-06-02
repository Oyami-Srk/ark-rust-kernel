use alloc::boxed::Box;
use alloc::{format, vec};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::borrow;
use core::cell::OnceCell;
use core::ops::Deref;
use fatfs::{DefaultTimeProvider, Dir, FileSystem, IoBase, LossyOemCpConverter, Read, Seek, SeekFrom, Write};
use log::info;
use crate::core::Spinlock;
use crate::filesystem::{DirEntry, DirEntryType, File, FileModes, FileOpenFlags, Filesystem, Inode, InodeStat, register_filesystem, SeekPosition};
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

#[derive(Copy, Clone, PartialEq)]
enum FatFSInodeType {
    Dir,
    File,
}

struct FatFSInode {
    inode_n: usize,
    path: String,
    type_: FatFSInodeType,
    mountpoint: Option<Arc<FileSystem<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>>>,
    fs: &'static FileSystem<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>,
}

impl Drop for FatFSInode {
    fn drop(&mut self) {}
}

impl Inode for FatFSInode {
    fn lookup(&self, name: &str, this_dentry: Weak<DirEntry>) -> Option<DirEntry> {
        let search_through_iter = |name: &str, path, dir: Dir<FatFSDeviceWrapper, DefaultTimeProvider, LossyOemCpConverter>, fs| -> Option<DirEntry> {
            if let Some(dir) = dir.iter().find(
                |v| if let Ok(v) = v { v.file_name() == name } else { false }) {
                let dir = dir.unwrap();
                let inode_n = dir.first_cluster().unwrap() as usize; // THIS IS OUR MODIFICATION TO FATFS
                let inode = FatFSInode {
                    inode_n,
                    path,
                    type_: if dir.is_dir() { FatFSInodeType::Dir } else { FatFSInodeType::File },
                    mountpoint: None,
                    fs,
                };
                let dentry = DirEntry {
                    parent: Some(this_dentry),
                    name: name.to_string(),
                    inode: Some(Arc::new(inode)),
                    type_: if dir.is_dir() { DirEntryType::Dir } else { DirEntryType::File },
                    children: Spinlock::new(BTreeMap::new()),
                    children_fully_loaded: OnceCell::new(),
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

    fn read_dir(&self, this_dentry: Weak<DirEntry>) -> crate::utils::error::Result<Vec<DirEntry>> {
        if self.type_ == FatFSInodeType::File {
            return Err("Cannot read dir on a file inode.".into());
        }
        let dir =
            if self.path == "/" {
                self.fs.root_dir()
            } else {
                self.fs.root_dir().open_dir(self.path.as_str()).unwrap()
            };
        let result = dir.iter()
            .filter_map(|possible_dirent| possible_dirent.ok())
            .map(|dirent| {
                let inode_n = dirent.first_cluster().unwrap_or(0) as usize; // THIS IS OUR MODIFICATION TO FATFS
                let inode = FatFSInode {
                    inode_n,
                    path: format!("{}/{}", self.path, dirent.file_name()),
                    type_: if dirent.is_dir() { FatFSInodeType::Dir } else { FatFSInodeType::File },
                    mountpoint: None,
                    fs: self.fs,
                };
                let dentry = DirEntry {
                    parent: Some(this_dentry.clone()),
                    name: dirent.file_name(),
                    inode: Some(Arc::new(inode)),
                    type_: if dirent.is_dir() { DirEntryType::Dir } else { DirEntryType::File },
                    children: Spinlock::new(BTreeMap::new()),
                    children_fully_loaded: OnceCell::new(),
                };
                dentry
            })
            .collect::<Vec<_>>();

        Ok(result)
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

    fn get_stat(&self) -> InodeStat {
        let type_bits = match &self.type_ {
            FatFSInodeType::Dir => { FileModes::DIRECTORY }
            FatFSInodeType::File => { FileModes::REGULAR }
        };
        let fs = self.fs;
        let dir = fs.root_dir();
        let possible_entry = dir.get_dentry(&self.path);
        InodeStat {
            ino: self.inode_n,
            mode: (type_bits | FileModes::RWX).bits() as usize,
            nlink: 1,
            size: possible_entry.map(|v| v.len() as usize).unwrap_or(0),
            block_size: self.fs.stats().unwrap().cluster_size() as usize,
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
            if this_read_bytes == 0 { break; }
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

    fn get_dentry(&self) -> KernelResult<Arc<DirEntry>> {
        Ok(self.dentry.clone())
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
        let fs = Arc::new(fs);
        let fs_ref = unsafe { Arc::as_ptr(&fs).as_ref::<'static>().unwrap() };
        let root_inode_n = (fs.stats().unwrap().total_clusters() + 1) as usize;
        Ok(Arc::new(FatFSInode {
            inode_n: root_inode_n,
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
