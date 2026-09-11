//! RV64 mstatus fields shared by privilege, memory and floating-point execution.

pub(crate) const SIE: u64 = 1 << 1;
pub(crate) const MIE: u64 = 1 << 3;
pub(crate) const SPIE: u64 = 1 << 5;
pub(crate) const MPIE: u64 = 1 << 7;
pub(crate) const SPP: u64 = 1 << 8;
pub(crate) const MPP: u64 = 3 << 11;
pub(crate) const FS: u64 = 3 << 13;
pub(crate) const XS: u64 = 3 << 15;
pub(crate) const MPRV: u64 = 1 << 17;
pub(crate) const SUM: u64 = 1 << 18;
pub(crate) const MXR: u64 = 1 << 19;
pub(crate) const TVM: u64 = 1 << 20;
pub(crate) const TW: u64 = 1 << 21;
pub(crate) const TSR: u64 = 1 << 22;
pub(crate) const UXL: u64 = 3 << 32;
pub(crate) const SD: u64 = 1 << 63;
