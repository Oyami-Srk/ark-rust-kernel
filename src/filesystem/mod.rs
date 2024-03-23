/* Structs */

mod vfs;

use crate::core::Mutex;
use core::iter::Peekable;
use core::cell::OnceCell;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::{Weak, Arc};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Display;
use core::ops::Deref;
use fatfs::Dir;
use log::info;
use lazy_static::lazy_static;
use crate::do_init;
use crate::utils::error::{Result, EmptyResult};

pub struct MountPoint {
    root: Arc<dyn Inode>,
    superblock: Arc<dyn Superblock>,
}

pub enum DirEntryType {
    File,
    Dir,
}

pub struct DirEntry {
    parent: Option<Weak<DirEntry>>,
    name: String,
    inode: Arc<dyn Inode>,
    type_: DirEntryType,
    children: Mutex<Vec<Arc<DirEntry>>>,
}

#[derive(Debug, Copy, Clone)]
pub enum FileMode {
    ReadWrite,
    ReadOnly,
    WriteOnly,
}

/* Traits */
pub trait Inode: Drop {
    // Inode must be droppable
    // 在目录项中寻找名字为name的。
    fn lookup(&self, name: &str) -> Option<DirEntry>;
    // 链接或取消链接一个inode到本Inode所指向的dir里面。
    fn link(&mut self, inode: &mut Arc<dyn Inode>, name: &str) -> EmptyResult;
    fn unlink(&self, name: &str) -> EmptyResult;
    // 创建/删除目录 inode
    fn mkdir(&mut self, name: &str) -> Result<Arc<dyn Inode>>;
    fn rmdir(&mut self, name: &str) -> EmptyResult;
    // 读取目录
    fn read_dir(&self) -> Result<Vec<DirEntry>>;
    // 开启文件
    fn open(&self) -> Result<Arc<dyn File>>;
}

// 1 FS has Many SBs
pub trait Superblock {
    fn alloc_inode(&mut self) -> Result<Arc<dyn Inode>>;
    fn root_inode(&self) -> Arc<dyn Inode>;
}

// 1 FS has ONE FS
pub trait Filesystem {
    fn new() -> Self where Self: Sized;
    fn mount(&self, device: Option<&mut dyn Inode>) -> Result<Arc<dyn Superblock>>;
}

pub trait File {
    fn get_offset(&self) -> usize;
    fn set_offset(&mut self, offset: usize) -> EmptyResult;
    fn get_mode(&self) -> FileMode;
    fn set_mode(&mut self, mode: FileMode) -> EmptyResult;

    fn seek(&mut self, offset: usize) -> EmptyResult;
    fn read(&mut self, offset: usize, len: usize) -> Result<&[u8]>;
    fn write(&mut self, offset: usize, buf: &[u8]) -> EmptyResult;

    fn open() -> Self;
    fn close(&mut self);
}

lazy_static! {
    static ref FILESYSTEMS: Mutex<BTreeMap<&'static str, Box<dyn Filesystem>>> = Mutex::new(BTreeMap::new());
}

const ROOT_INODE: OnceCell<Arc<dyn Inode>> = OnceCell::new();
const ROOT_DENTRY: OnceCell<Arc<DirEntry>> = OnceCell::new();

pub fn register_filesystem(name: &'static str, filesystem: Box<dyn Filesystem>) {
    FILESYSTEMS.lock().insert(name, filesystem);
}

pub fn init() {
    info!("Initializing Filesystem");
    do_init!(vfs);
    let filesystems = FILESYSTEMS.lock();
    let root_inode = ROOT_INODE.get_or_init(|| {
        let virt_fs = filesystems.get("vfs").unwrap();
        let root_sb = virt_fs.mount(None).unwrap();
        root_sb.root_inode()
    }).clone();

    ROOT_DENTRY.get_or_init(|| {
        Arc::new(DirEntry {
            parent: None,
            name: "/".to_string(),
            inode: root_inode.clone(),
            children: Mutex::new(Vec::new()),
            type_: DirEntryType::Dir,
        })
    });
}

/*
    Filesystem子系统负责管理DirEntry。其他部分交由具体的FS实现Inode和File部分。
 */

pub fn get_parent_dir_entry_and_path_basename(path: &str, cwd: Option<Arc<DirEntry>>) -> Option<(Arc<DirEntry>, &str)> {
    let root_dentry_arc = ROOT_DENTRY.get().unwrap().clone();
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
                let dir_inode = parent.inode.clone();
                let lookup_result = dir_inode.lookup(name);
                if let Some(lookup_result) = lookup_result {
                    let dentry = Arc::new(lookup_result);
                    parent.children.lock().push(dentry.clone());
                    new_parent = Some(dentry);
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
    let parent = get_parent_dir_entry_and_path_basename(path, cwd);
    if let Some((parent, target_name)) = parent {
        if target_name == ".." {
            Some(parent.parent.as_ref().map(|p| p.upgrade().unwrap()).unwrap_or(ROOT_DENTRY.get().unwrap().clone()))
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
                let dir_inode = parent.inode.clone();
                let lookup_result = dir_inode.lookup(target_name);
                if let Some(lookup_result) = lookup_result {
                    let dentry = Arc::new(lookup_result);
                    parent.children.lock().push(dentry.clone());
                    return Some(dentry);
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
