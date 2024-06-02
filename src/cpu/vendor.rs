use alloc::sync::Arc;
use lazy_static::lazy_static;

const VENDOR_ID_ANDES: usize = 0x31e;
const VENDOR_ID_SI_FIVE: usize = 0x489;
const VENDOR_ID_THEAD: usize = 0x5b7;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(usize)]
pub enum VendorId {
    Unknown(usize),
    ANDES = VENDOR_ID_ANDES,
    SiFive = VENDOR_ID_SI_FIVE,
    THead = VENDOR_ID_THEAD,
}

impl From<usize> for VendorId {
    fn from(value: usize) -> Self {
        match value {
            VENDOR_ID_ANDES => Self::ANDES,
            VENDOR_ID_SI_FIVE => Self::SiFive,
            VENDOR_ID_THEAD => Self::THead,
            v => Self::Unknown(v)
        }
    }
}

pub type ArchId = usize;
pub type ImplId = usize;

pub struct CpuId(VendorId, ArchId, ImplId);

lazy_static! {
    pub static ref CPUID: CpuId = CpuId::fetch();
}

impl CpuId {
    pub fn fetch() -> Self {
        let mvendorid = VendorId::from(sbi::base::mvendorid());
        let marchid = sbi::base::marchid();
        let mimplid = sbi::base::mimpid();
        Self(mvendorid, marchid, mimplid)
    }

    pub fn get_vendor(&self) -> VendorId {
        self.0
    }

    pub fn get_arch(&self) -> ArchId {
        self.1
    }

    pub fn get_impl(&self) -> ImplId {
        self.2
    }
}