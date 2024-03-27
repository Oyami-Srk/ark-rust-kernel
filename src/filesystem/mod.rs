/* Structs */

use crate::core::Spinlock;
use core::iter::Peekable;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::{Weak, Arc};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::OnceCell;
use core::fmt::{Debug, Display, Formatter};
use core::ops::Deref;
use bitflags::{bitflags, Flags};
use fatfs::{Dir, Seek};
use log::info;
use lazy_static::lazy_static;
use num_derive::{FromPrimitive, ToPrimitive};
use crate::{do_init, println};
use crate::utils::error::{Result, EmptyResult};

#[derive(Copy, Clone)]
pub enum DirEntryType {
    File,
    Dir,
}

pub struct DirEntry {
    parent: Option<Weak<DirEntry>>,
    name: String,
    inode: Option<Arc<dyn Inode>>,
    type_: DirEntryType,
    children: Spinlock<Vec<Arc<DirEntry>>>,
}

impl Debug for DirEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "[DirEntry({}) {}]", match &self.type_ {
            &DirEntryType::File => "File",
            &DirEntryType::Dir => "Dir"
        }, get_dentry_fullpath(self))
    }
}

bitflags! {
    pub struct FileOpenFlags: u32 {
        const ReadOnly = 0x00;
        const WriteOnly = 0x01;
        const ReadWrite = 0x02;
        const Create = 0x40;
        const Directory = 0x0200000;
    }
}

bitflags! {
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
        !self.contains(FileOpenFlags::WriteOnly)
    }

    pub fn is_write(&self) -> bool {
        !self.contains(FileOpenFlags::ReadOnly)
    }

    pub fn is_directory(&self) -> bool {
        self.contains(FileOpenFlags::Directory)
    }

    pub fn is_create(&self) -> bool {
        self.contains(FileOpenFlags::Create)
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
}

#[derive(Debug, Copy, Clone, FromPrimitive, ToPrimitive)]
pub enum SeekPosition {
    Set = 0,
    Cur = 1,
    End = 2,
}

/* Traits */
pub trait Inode: Drop {
    // Inode must be droppable
    // 在目录项中寻找名字为name的。
    fn lookup(&self, name: &str) -> Option<DirEntry>;
    // 链接或取消链接一个inode到本Inode所指向的dir里面。
    fn link(&self, inode: Arc<dyn Inode>, name: &str) -> EmptyResult;
    fn unlink(&self, name: &str) -> EmptyResult;
    // 创建/删除目录 inode
    fn mkdir(&self, name: &str) -> Result<Arc<dyn Inode>>;
    fn rmdir(&self, name: &str) -> EmptyResult;
    // 读取目录
    fn read_dir(&self) -> Result<Vec<DirEntry>>;
    // 开启文件
    fn open(&self, dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> Result<Arc<dyn File>>;
    // 获取DirEntry类型
    fn get_dentry_type(&self) -> DirEntryType;
}

pub trait Superblock {
    fn alloc_inode(&mut self, type_: DirEntryType) -> Result<Arc<dyn Inode>>;
}

// 1 FS has ONE FS
pub trait Filesystem {
    fn new() -> Self where Self: Sized;
    fn mount(&self, device: Option<Arc<dyn File>>) -> Result<Arc<dyn Inode>>;
}

pub trait File {
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize>;
    fn read(&self, len: usize) -> Result<Vec<u8>>;
    fn write(&self, buf: &[u8]) -> Result<usize>;
    fn close(&mut self);
    fn get_dentry(&self) -> Arc<DirEntry>;
}

pub struct DirFile {
    dentry: Arc<DirEntry>,
    iterator: Spinlock<Option<usize>>,
}

impl File for DirFile {
    fn seek(&self, offset: isize, whence: SeekPosition) -> Result<usize> {
        let mut iterator = self.iterator.lock();
        if offset <= 0 {
            *iterator = None;
        } else {
            *iterator = Some(offset as usize);
        }
        Ok(iterator.unwrap_or(0))
    }

    fn read(&self, len: usize) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        Ok(0)
    }

    fn close(&mut self) {}

    fn get_dentry(&self) -> Arc<DirEntry> {
        self.dentry.clone()
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
        children: Spinlock::new(Vec::new()),
        type_: DirEntryType::Dir,
    });
    // Safety: Only write here once
    unsafe { ROOT_DENTRY = Some(root_dentry) };
}

/*
    Filesystem子系统负责管理DirEntry。其他部分交由具体的FS实现Inode和File部分。
 */

pub fn get_parent_dentry(path: &str, cwd: Option<Arc<DirEntry>>) -> Option<(Arc<DirEntry>, &str)> {
    let root_dentry_arc = get_root();
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
                let children = parent.children.lock();
                for child in children.iter() {
                    if child.name == name {
                        new_parent = Some(child.clone());
                        break 'found;
                    }
                }

                // not found in loaded children
                if let Some(dir_inode) = &parent.inode {
                    let dir_inode = dir_inode.clone();
                    let lookup_result = dir_inode.lookup(name);
                    if let Some(lookup_result) = lookup_result {
                        let dentry = Arc::new(lookup_result);
                        parent.children.lock().push(dentry.clone());
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

pub fn get_dentry(path: &str, cwd: Option<Arc<DirEntry>>) -> Option<Arc<DirEntry>> {
    let parent = get_parent_dentry(path, cwd);
    if let Some((parent, target_name)) = parent {
        if target_name == ".." {
            Some(parent.parent.as_ref().map(|p| p.upgrade().unwrap()).unwrap_or(get_root()))
        } else if target_name == "." {
            Some(parent)
        } else {
            loop {
                let children = parent.children.lock();
                for child in children.iter() {
                    if child.name == target_name {
                        return Some(child.clone());
                    }
                }

                // not found in loaded children
                if let Some(dir_inode) = &parent.inode {
                    let dir_inode = dir_inode.clone();
                    let lookup_result = dir_inode.lookup(target_name);
                    if let Some(lookup_result) = lookup_result {
                        let dentry = Arc::new(lookup_result);
                        parent.children.lock().push(dentry.clone());
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

pub fn get_dentry_fullpath(dentry: &DirEntry) -> String {
    let mut path = String::new();
    while let Some(dentry) = &dentry.parent {
        let mut new_path = String::from(&dentry.upgrade().unwrap().name);
        new_path.push('/');
        new_path.push_str(path.as_str());
        path = new_path;
    }
    path
}

pub fn link(parent: Arc<DirEntry>, inode: Arc<dyn Inode>, name: &str) -> Result<Arc<DirEntry>> {
    let children = parent.children.lock();
    if children.iter().any(|v| v.name == name) {
        return Err("Already existed.".into());
    }

    if let Some(inode) = &parent.inode {
        inode.link(inode.clone(), name).expect("Cannot link to parent inode")
    }


    let dentry = Arc::new(DirEntry {
        parent: Some(Arc::downgrade(&parent)),
        name: name.to_string(),
        inode: Some(inode.clone()),
        type_: inode.get_dentry_type(),
        children: Spinlock::new(Vec::new()),
    });
    parent.children.lock().push(dentry.clone());

    Ok(dentry)
}

pub fn open(dentry: Arc<DirEntry>, flags: FileOpenFlags, mode: FileModes) -> Result<Arc<dyn File>> {
    match dentry.type_ {
        DirEntryType::File => {
            if let Some(inode) = &dentry.inode {
                inode.open(dentry.clone(), flags, mode)
            } else {
                Err("No inode to open".into())
            }
        }
        DirEntryType::Dir => Ok(Arc::new(DirFile {
            dentry,
            iterator: Spinlock::new(None),
        }))
    }
}

pub fn close(file: Arc<dyn File>) -> EmptyResult {
    Ok(())
}

pub fn read(file: Arc<dyn File>, len: usize) -> Result<Vec<u8>> {
    file.read(len)
}

pub fn write(file: Arc<dyn File>, buffer: &[u8]) -> Result<usize> {
    file.write(buffer)
}

pub fn lseek(file: Arc<dyn File>, offset: isize, whence: SeekPosition) -> Result<usize> {
    file.seek(offset, whence)
}

pub fn mkdir(parent: Arc<DirEntry>, name: &str) -> Result<Arc<DirEntry>> {
    if name == "." || name == ".." {
        return Err("Try to mkdir of parent or self.".into());
    }
    let children = parent.children.lock();
    if children.iter().any(|v| v.name == name) {
        return Err("Already existed.".into());
    }
    let mut dir_inode = if let Some(inode) = &parent.inode {
        Some(inode.mkdir(name).expect("Failed to mkdir inode."))
    } else {
        None
    };

    Ok(Arc::new(DirEntry {
        parent: Some(Arc::downgrade(&parent)),
        name: name.to_string(),
        inode: dir_inode,
        type_: DirEntryType::Dir,
        children: Spinlock::new(Vec::new()),
    }))
}

pub fn read_dir(dir: Arc<DirFile>) -> Option<Arc<DirEntry>> {
    todo!()
}

pub fn get_root() -> Arc<DirEntry> {
    return unsafe { ROOT_DENTRY.as_ref().unwrap() }.clone();
}
