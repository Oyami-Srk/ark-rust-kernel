/* Structs */

mod fatfs;

use crate::core::Spinlock;
use core::iter::Peekable;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::{Weak, Arc};
use alloc::{format, vec};
use alloc::vec::Vec;
use core::cell::OnceCell;
use core::fmt::{Debug, Display, Formatter};
use core::ops::Deref;
use ::fatfs::Dir;
use bitflags::{bitflags, Flags};
use log::info;
use lazy_static::lazy_static;
use sbi::pmu::configure_matching_counters;
use virtio_drivers::device::socket::SocketError;
use crate::{do_init, println};
use crate::utils::error::{Result, EmptyResult};

#[derive(Copy, Clone, PartialEq)]
pub enum DirEntryType {
    File,
    Dir,
}

pub struct DirEntry {
    parent: Option<Weak<DirEntry>>,
    pub name: String,
    inode: Option<Arc<dyn Inode>>,
    type_: DirEntryType,
    children: Spinlock<BTreeMap<String, Arc<DirEntry>>>,
    children_fully_loaded: OnceCell<()>,
}

impl Debug for DirEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "[DirEntry({}) {}]", match &self.type_ {
            &DirEntryType::File => "File",
            &DirEntryType::Dir => "Dir"
        }, self.fullpath())
    }
}

bitflags! {
    pub struct FileOpenFlags: u32 {
        const O_RDONLY = 0x00;
        const O_WRONLY = 0x01;
        const O_RDWR = 0x02;

        // file creation flags
        const O_CREAT = 0x40;
        const O_EXCL = 0x80;
        const O_TRUNC = 0x200;
        const O_DIRECTORY = 0x10000;
        const O_CLOEXEC = 0x80000;

        // file status flags
        const O_APPEND = 0x400;
        const O_NONBLOCK = 0x800;
        const O_LARGEFILE = 0x8000;
        const O_PATH = 0x200000;

    }
}

bitflags! {
    #[derive(Copy, Clone, PartialEq)]
    pub struct FileModes: u32 {
        const OwnerRead = 0o400;
        const OwnerWrite = 0o200;
        const OwnerExec = 0o100;
        const GroupRead = 0o040;
        const GroupWrite = 0o020;
        const GroupExec = 0o010;
        const OtherRead = 0o004;
        const OtherWrite = 0o002;
        const OtherExec = 0o001;

        const Read = Self::OwnerRead.bits() | Self::GroupRead.bits() | Self::OtherRead.bits();
        const Write = Self::OwnerWrite.bits() | Self::GroupWrite.bits() | Self::OtherWrite.bits();
        const Exec = Self::OwnerExec.bits() | Self::GroupExec.bits() | Self::OtherExec.bits();
        const RWX = Self::Read.bits() | Self::Write.bits() | Self::Exec.bits();

        // musl: include/sys/stat.h
        const REGULAR = 0o100_000;
        const LINK = 0o120_000;
        const SOCKET = 0o140_000;

        const FIFO = 0o10_000;
        const CHAR = 0o20_000;
        const DIRECTORY = 0o40_000;
        const BLK = 0o60_000;
    }
}

impl From<usize> for FileOpenFlags {
    fn from(value: usize) -> Self {
        FileOpenFlags::from_bits(value as u32).unwrap()
    }
}

impl From<usize> for FileModes {
    fn from(value: usize) -> Self {
        FileModes::from_bits(value as u32).unwrap()
    }
}

impl FileOpenFlags {
    pub fn is_read(&self) -> bool {
        !self.contains(FileOpenFlags::O_WRONLY)
    }

    pub fn is_write(&self) -> bool {
        !self.contains(FileOpenFlags::O_RDONLY)
    }

    pub fn is_directory(&self) -> bool {
        self.contains(FileOpenFlags::O_DIRECTORY)
    }

    pub fn is_create(&self) -> bool {
        self.contains(FileOpenFlags::O_CREAT)
    }

    pub fn must_create(&self) -> bool {
        self.contains(FileOpenFlags::O_CREAT) && self.contains(FileOpenFlags::O_EXCL)
    }
}

impl FileModes {
    pub fn owner(&self) -> (bool, bool, bool) {
        (self.contains(FileModes::OwnerRead), self.contains(FileModes::OwnerWrite), self.contains(FileModes::OwnerExec))
    }

    pub fn group(&self) -> (bool, bool, bool) {
        (self.contains(FileModes::GroupRead), self.contains(FileModes::GroupWrite), self.contains(FileModes::GroupExec))
    }

    pub fn other(&self) -> (bool, bool, bool) {
        (self.contains(FileModes::OtherRead), self.contains(FileModes::OtherWrite), self.contains(FileModes::OtherExec))
    }

    pub fn is_read(&self) -> bool {
        self.contains(Self::OwnerRead) | self.contains(Self::GroupRead) | self.contains(Self::OtherRead)
    }

    pub fn is_write(&self) -> bool {
        self.contains(Self::OwnerWrite) | self.contains(Self::GroupWrite) | self.contains(Self::OtherWrite)
    }

    pub fn is_exec(&self) -> bool {
        self.contains(Self::OwnerExec) | self.contains(Self::GroupExec) | self.contains(Self::OtherExec)
    }

    pub fn mask_file_type(&self) -> Self {
        Self::from_bits(self.bits() & 0o170_000).unwrap()
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SeekPosition {
    Set = 0,
    Cur = 1,
    End = 2,
}

#[derive(Debug, Clone)]
pub struct InodeStat {
    pub ino: usize,
    pub mode: usize,
    pub nlink: usize,
    pub size: usize,
    pub block_size: usize,
}

impl InodeStat {
    pub fn vfs_inode_stat() -> Self {
        InodeStat {
            ino: 0,
            mode: (FileModes::DIRECTORY | FileModes::Read | FileModes::Write | FileModes::Exec).bits() as usize,
            nlink: 1,
            size: 0,
            block_size: 0,
        }
    }
}

/* Traits */
pub trait Inode: Drop {
    // Inode must be droppable
    // 在目录项中寻找名字为name的。
    fn lookup(&self, name: &str, this_dentry: Weak<DirEntry>) -> Option<DirEntry>;
    // 链接或取消链接一个inode到本Inode所指向的dir里面。
    fn link(&self, inode: Arc<dyn Inode>, name: &str) -> EmptyResult;
    fn unlink(&self, name: &str) -> EmptyResult;
    // 创建/删除目录 inode
    fn mkdir(&self, name: &str) -> Result<Arc<dyn Inode>>;
    fn rmdir(&self, name: &str) -> EmptyResult;
    // 读取目录
    fn read_dir(&self, this_dentry: Weak<DirEntry>) -> Result<Vec<DirEntry>>;
    // 开启文件
    fn open(&self, dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> Result<Arc<dyn File>>;
    // 获取DirEntry类型
    fn get_dentry_type(&self) -> DirEntryType;
    // 获取统计信息
    fn get_stat(&self) -> InodeStat;
}

pub trait Superblock {
    fn alloc_inode(&mut self, type_: DirEntryType) -> Result<Arc<dyn Inode>>;
}

// 1 FS has ONE FS
pub trait Filesystem {
    fn new() -> Self where Self: Sized;
    fn mount(&self, device: Option<Arc<dyn File>>, mount_point: Arc<DirEntry>) -> Result<Arc<dyn Inode>>;
}

pub trait File {
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize>;
    fn read(&self, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, buf: &[u8]) -> Result<usize>;
    fn close(&self) -> EmptyResult;
    fn get_dentry(&self) -> Result<Arc<DirEntry>>;
}

pub struct DirFile {
    dentry: Arc<DirEntry>,
    iterator: Spinlock<usize>,
}

impl File for DirFile {
    // DirFile is just a position holder
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize> {
        let mut iterator = self.iterator.lock();
        match whence {
            SeekPosition::Set => {
                if offset <= 0 {
                    *iterator = 0;
                } else {
                    *iterator = offset as usize;
                }
            }
            SeekPosition::Cur => {
                if offset < 0 {
                    *iterator = iterator.saturating_sub(offset.unsigned_abs());
                } else {
                    *iterator += offset.unsigned_abs();
                }
            }
            SeekPosition::End => {
                return Err("Cannot seek from end in DirFile".into());
            }
        }
        Ok(*iterator)
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        Ok(0)
    }

    fn close(&self) -> EmptyResult { Ok(()) }

    fn get_dentry(&self) -> Result<Arc<DirEntry>> {
        Ok(self.dentry.clone())
    }
}

lazy_static! {
    static ref FILESYSTEMS: Spinlock<BTreeMap<&'static str, Box<dyn Filesystem>>> = Spinlock::new(BTreeMap::new());
}

static mut ROOT_DENTRY: Option<Arc<DirEntry>> = None;

pub fn register_filesystem(name: &'static str, filesystem: Box<dyn Filesystem>) {
    FILESYSTEMS.lock().insert(name, filesystem);
}

pub fn init() {
    info!("Initializing Filesystem");
    let root_dentry = Arc::new(DirEntry {
        parent: None,
        name: "/".to_string(),
        inode: None,
        children: Spinlock::new(BTreeMap::new()),
        type_: DirEntryType::Dir,
        children_fully_loaded: OnceCell::new(),
    });
    // Safety: Only write here once
    unsafe { ROOT_DENTRY = Some(root_dentry.clone()) };
    // Create /dev
    root_dentry.mkdir("dev").expect("Failed to create /dev on vfs.");

    do_init!(
        fatfs
    );
}

pub fn mount(cwd: Option<Arc<DirEntry>>, dev: &str, mount_point: &str, filesystem: &str) -> EmptyResult {
    // get filesystem
    let fss = FILESYSTEMS.lock();
    let fs = fss.get(filesystem).ok_or("Filesystem Not Found")?;
    // get dev
    let dev = DirEntry::from_path(dev, cwd.clone()).ok_or("Device Not Found")?;
    // get mount_point
    let mut mount_point = DirEntry::from_path(mount_point, cwd.clone()).ok_or("Mount Point Not Found")?;
    // check if mount_point is a dir
    if mount_point.type_ != DirEntryType::Dir {
        return Err("Mount Point is not a directory.".into());
    }
    // check if mount_point is empty
    if mount_point.children.lock().len() != 0 {
        return Err("Mount Point is not empty.".into());
    }
    // FIXME: Check if already mounted.

    // Open device file
    let dev = dev.open(FileOpenFlags::O_RDWR, FileModes::from_bits(0).unwrap())?;
    // mount filesystem
    let root_inode = fs.mount(Some(dev), mount_point.clone())?;
    // mount to dentry
    unsafe {
        Arc::get_mut_unchecked(&mut mount_point).inode = Some(root_inode);
    }
    Ok(())
}

/*
    Filesystem子系统负责管理DirEntry。其他部分交由具体的FS实现Inode和File部分。
 */
impl DirEntry {
    pub fn new(parent: Option<Weak<DirEntry>>, name: String, inode: Option<Arc<dyn Inode>>, type_: DirEntryType) -> Self {
        Self {
            parent,
            name,
            inode,
            type_,
            children: Spinlock::new(BTreeMap::new()),
            children_fully_loaded: OnceCell::new(),
        }
    }

    pub fn root() -> Arc<DirEntry> {
        // Safety: ROOT_DENTRY is immutable after fs::init
        return unsafe { ROOT_DENTRY.as_ref().unwrap() }.clone();
    }

    pub fn get_parent(path: &str, cwd: Option<Arc<DirEntry>>) -> Option<(Arc<DirEntry>, &str)> {
        let root_dentry_arc = Self::root();
        let (cwd, path) = if let Some(cwd) = cwd && !path.starts_with("/") {
            (cwd, path)
        } else {
            (root_dentry_arc.clone(), if path.starts_with("/") {
                &path[1..]
            } else {
                path
            })
        };

        let mut paths = path.split("/").peekable();
        let mut parent = cwd;
        while let Some(name) = paths.next() {
            if paths.peek().is_none() {
                // last name
                return Some((parent, name));
            }
            if name == ".." {
                parent = parent.parent.as_ref().map(|p| p.upgrade().expect("Parent not found."))
                    .unwrap_or(root_dentry_arc.clone());
            } else if name == "." || name.len() == 0 {
                // do nothing
            } else {
                // do search deep
                let mut new_parent = None;
                'found: loop {
                    if DirEntryType::Dir != parent.type_ {
                        return None;
                    }
                    let mut children = parent.children.lock();
                    if let Some(child) = children.get(name) {
                        new_parent = Some(child.clone());
                        break 'found;
                    }

                    // not found in loaded children
                    if let Some(dir_inode) = &parent.inode {
                        let dir_inode = dir_inode.clone();
                        let lookup_result = dir_inode.lookup(name, Arc::downgrade(&parent));
                        if let Some(mut lookup_result) = lookup_result {
                            let dentry = Arc::new(lookup_result);
                            children.insert(dentry.name.clone(), dentry.clone());
                            new_parent = Some(dentry);
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                if let Some(new_parent) = new_parent {
                    parent = new_parent
                }
            }
        }
        // If path is empty
        Some((parent, ""))
    }

    pub fn from_path(path: &str, cwd: Option<Arc<DirEntry>>) -> Option<Arc<DirEntry>> {
        if path == "/" {
            return Some(Self::root());
        }
        let parent = Self::get_parent(path, cwd);
        if let Some((parent, target_name)) = parent {
            if target_name == ".." {
                Some(parent.parent.as_ref().map(|p| p.upgrade().unwrap()).unwrap_or(Self::root()))
            } else if target_name == "." || target_name == "" {
                Some(parent)
            } else {
                loop {
                    let mut children = parent.children.lock();

                    if let Some(child) = children.get(target_name) {
                        return Some(child.clone());
                    }

                    // not found in loaded children
                    if let Some(dir_inode) = &parent.inode {
                        let dir_inode = dir_inode.clone();
                        let lookup_result = dir_inode.lookup(target_name, Arc::downgrade(&parent));
                        if let Some(lookup_result) = lookup_result {
                            let dentry = Arc::new(lookup_result);
                            children.insert(dentry.name.clone(), dentry.clone());
                            return Some(dentry);
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn fullpath(&self) -> String {
        let mut path = String::new();
        let mut cur = self;
        while let Some(dentry) = &cur.parent {
            let d = dentry.upgrade().unwrap();
            let mut new_path = String::from(&d.name);
            if &d.name != "/" {
                new_path.push('/');
            }
            new_path.push_str(path.as_str());
            path = new_path;
            cur = unsafe { (d.as_ref() as *const DirEntry).as_ref().unwrap() };
        }
        format!("{}{}", path, self.name)
    }

    pub fn link(self: Arc<Self>, inode: Arc<dyn Inode>, name: &str) -> Result<Arc<DirEntry>> {
        let mut children = self.children.lock();
        if children.contains_key(name) {
            return Err("Already existed.".into());
        }

        if let Some(inode) = &self.inode {
            inode.link(inode.clone(), name).expect("Cannot link to parent inode")
        }


        let dentry = Arc::new(DirEntry {
            parent: Some(Arc::downgrade(&self)),
            name: name.to_string(),
            inode: Some(inode.clone()),
            type_: inode.get_dentry_type(),
            children: Spinlock::new(BTreeMap::new()),
            children_fully_loaded: OnceCell::new(),
        });
        children.insert(name.to_string(), dentry.clone());

        Ok(dentry)
    }

    pub fn open(self: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> Result<Arc<dyn File>> {
        match self.type_ {
            DirEntryType::File => {
                self.inode.as_ref().ok_or("No inode to open")?.open(self.clone(), flags, mode)
            }
            DirEntryType::Dir => Ok(Arc::new(DirFile {
                dentry: self,
                iterator: Spinlock::new(0),
            }))
        }
    }

    pub fn mkdir(self: Arc<DirEntry>, name: &str) -> Result<Arc<DirEntry>> {
        if name == "." || name == ".." {
            return Err("Try to mkdir of parent or self.".into());
        }
        let mut children = self.children.lock();
        if children.contains_key(name) {
            return Err("Already existed.".into());
        }
        let mut dir_inode = if let Some(inode) = &self.inode {
            Some(inode.mkdir(name).expect("Failed to mkdir inode."))
        } else {
            None
        };

        let dentry = Arc::new(DirEntry {
            parent: Some(Arc::downgrade(&self)),
            name: name.to_string(),
            inode: dir_inode,
            type_: DirEntryType::Dir,
            children: Spinlock::new(BTreeMap::new()),
            children_fully_loaded: OnceCell::new(),
        });
        children.insert(name.to_string(), dentry.clone());
        Ok(dentry)
    }

    pub fn get_inode(&self) -> Option<Arc<dyn Inode>> {
        self.inode.clone()
    }

    pub fn get_child(self: &Arc<Self>, i: usize) -> Result<Option<Arc<DirEntry>>> {
        if self.children_fully_loaded.get().is_none() {
            // Not FULLY loaded yet
            if let Some(inode) = &self.inode {
                let children = inode.read_dir(Arc::downgrade(self))?;
                // dedup
                self.children.lock().extend(children.into_iter().map(|v| (v.name.clone(), Arc::new(v))));
            }
            // VFS always FULLY loaded.
            self.children_fully_loaded.set(()).unwrap();
        }
        let children = self.children.lock();
        let mut iter = children.iter();
        for i in 0..i {
            iter.next();
        }
        Ok(iter.next().map(|(k, v)| v.clone()))
    }
}
