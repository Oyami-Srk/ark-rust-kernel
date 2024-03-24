use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use crate::core::Mutex;
use crate::filesystem::{DirEntry, DirEntryType, File, Filesystem, Inode, Superblock};
use crate::utils::error::{EmptyResult, Result};

struct VFSData {
    mounted: bool,
}

struct VirtualFilesystem {
    data: Mutex<VFSData>,
}

impl VFSData {
    pub fn new() -> Self {
        Self {
            mounted: false
        }
    }
}

impl Filesystem for VirtualFilesystem {
    fn new() -> Self where Self: Sized {
        VirtualFilesystem {
            data: Mutex::new(VFSData::new()),
        }
    }

    fn mount(&self, device: Option<&mut dyn Inode>) -> Result<Arc<dyn Superblock>> {
        let mut data = self.data.lock();
        if data.mounted == true {
            panic!("VFS can be only mounted once.");
        } else {
            data.mounted = true;
            Ok(VFSSuperblock::new())
        }
    }
}

struct VFSSuperblock {
    root_inode: Arc<VFSInode>,
    inodes: Vec<Arc<VFSInode>>
}

impl VFSSuperblock {
    pub fn new() -> Arc<Self> {
        let root_inode_data = VFSInodeData{
            n_links: 1,
            n_files: 0,
            n_byte_size: 0,
        };
        let root_inode = Arc::new(VFSInode {
            type_: InodeType::Dir,
            data: Mutex::new(root_inode_data)
        });
        Arc::new(Self {
            root_inode: root_inode,
            inodes: vec![],
        })
    }
}

impl Superblock for VFSSuperblock{
    fn alloc_inode(&mut self, type_: DirEntryType) -> Result<Arc<dyn Inode>> {
        let inode_data = VFSInodeData {
            n_links: 1,
            n_files: 0,
            n_byte_size: 0,
        };

        Ok(Arc::new(VFSInode {
            type_: match type_ {
                DirEntryType::File => InodeType::File,
                DirEntryType::Dir => InodeType::Dir
            },
            data: Mutex::new(inode_data),
        }))
    }

    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root_inode.clone()
    }
}

enum InodeType {
    Dir,
    File,
    MountPoint
}

struct VFSInode {
    // Since VFS Inode is not persistent, we haven't to got even an id
    type_: InodeType,
    data: Mutex<VFSInodeData>
}

struct VFSInodeData {
    n_links: usize,
    n_files: usize,
    n_byte_size: usize,
}

impl Drop for VFSInode {
    fn drop(&mut self) {
        // do nothing
    }
}

impl Inode for VFSInode {
    fn lookup(&self, name: &str) -> Option<DirEntry> {
        todo!()
    }

    fn link(&mut self, inode: &mut Arc<dyn Inode>, name: &str) -> EmptyResult {
        todo!()
    }

    fn unlink(&self, name: &str) -> EmptyResult {
        todo!()
    }

    fn mkdir(&mut self, name: &str) -> Result<Arc<dyn Inode>> {
        todo!()
    }

    fn rmdir(&mut self, name: &str) -> EmptyResult {
        todo!()
    }

    fn read_dir(&self) -> Result<Vec<DirEntry>> {
        todo!()
    }

    fn open(&self) -> Result<Arc<dyn File>> {
        todo!()
    }
}

pub fn init() {
    super::register_filesystem("vfs", Box::new(VirtualFilesystem::new()));
}