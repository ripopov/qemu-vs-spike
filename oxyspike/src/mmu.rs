use crate::arch::{access, exception, privilege};
use crate::{Cpu, Trap, csr, sext, status};
pub(crate) const TLB_ENTRIES: usize = 1024;
const PAGE_SHIFT: u32 = 12;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
const PAGE_OFFSET_MASK: u64 = PAGE_SIZE as u64 - 1;
const VPN_BITS: u32 = 9;
const VPN_MASK: u64 = (1 << VPN_BITS) - 1;
const PPN_MASK: u64 = (1 << 44) - 1;

const PTE_VALID: u64 = 1 << 0;
const PTE_READ: u64 = 1 << 1;
const PTE_WRITE: u64 = 1 << 2;
const PTE_EXECUTE: u64 = 1 << 3;
const PTE_USER: u64 = 1 << 4;
const PTE_ACCESSED: u64 = 1 << 6;
const PTE_DIRTY: u64 = 1 << 7;
#[derive(Clone, Copy)]
pub(crate) struct TlbEntry {
    tag: u64,
    satp: u64,
    physical_page: u64,
}

impl TlbEntry {
    pub const EMPTY: Self = Self {
        tag: u64::MAX,
        satp: 0,
        physical_page: 0,
    };
}

impl Cpu {
    pub fn flush_translation(&mut self) {
        self.flush_instruction();
        for cache in &mut self.tlb {
            cache.fill(TlbEntry::EMPTY);
        }
    }

    #[inline(always)]
    fn cached_translate(&mut self, va: u64, size: usize, access: u8) -> Result<u64, Trap> {
        let privilege = self.effective_privilege(access);
        // Page-offset bits hold privilege and the SUM/MXR permission context.
        let tag =
            (va & !PAGE_OFFSET_MASK) | (privilege as u64) | ((self.csrs[csr::MSTATUS] >> 16) & 12);
        let satp = if privilege == privilege::MACHINE {
            0
        } else {
            self.csrs[csr::SATP]
        };
        let index = ((va >> PAGE_SHIFT) as usize) & (TLB_ENTRIES - 1);
        let e = self.tlb[access as usize][index];
        if e.tag == tag && e.satp == satp {
            return Ok(e.physical_page | (va & PAGE_OFFSET_MASK));
        }
        let pa = self.translate(va, size, access)?;
        // A PMP boundary within a page must never be hidden by a cached hit.
        if self.pmp_check(pa & !PAGE_OFFSET_MASK, PAGE_SIZE, access, privilege) {
            self.tlb[access as usize][index] = TlbEntry {
                tag,
                satp,
                physical_page: pa & !PAGE_OFFSET_MASK,
            };
        }
        Ok(pa)
    }

    fn effective_privilege(&self, access: u8) -> u8 {
        if access != access::FETCH
            && self.privilege == privilege::MACHINE
            && self.csrs[csr::MSTATUS] & status::MPRV != 0
        {
            ((self.csrs[csr::MSTATUS] >> 11) & 3) as u8
        } else {
            self.privilege
        }
    }
    pub(crate) fn pmp_check(&self, addr: u64, size: usize, access: u8, privilege: u8) -> bool {
        let end = match addr.checked_add(size as u64) {
            Some(v) => v,
            None => return false,
        };
        for n in 0..16 {
            let cfg = self.pmp_cfg(n);
            let mode = (cfg >> 3) & 3;
            let a = self.csrs[csr::PMPADDR0 + n];
            let (lo, hi) = match mode {
                0 => continue,
                1 => (
                    if n == 0 {
                        0
                    } else {
                        self.csrs[csr::PMPADDR0 + n - 1] << 2
                    },
                    a << 2,
                ),
                2 => (a << 2, (a << 2) + 4),
                _ => {
                    let bits = a.trailing_ones();
                    let mask = (1u64 << (bits + 3)) - 1;
                    (
                        (a << 2) & !mask,
                        ((a << 2) & !mask).saturating_add(mask + 1),
                    )
                }
            };
            if addr < hi && end > lo {
                return addr >= lo
                    && end <= hi
                    && (privilege == privilege::MACHINE && cfg & 128 == 0
                        || cfg
                            & (1 << match access {
                                access::FETCH => 2,
                                access::LOAD => 0,
                                _ => 1,
                            })
                            != 0);
            }
        }
        privilege == privilege::MACHINE
    }

    /// Translate one contiguous access; `access` is an `arch::access` encoding.
    pub fn translate(&mut self, va: u64, size: usize, access: u8) -> Result<u64, Trap> {
        let privilege = self.effective_privilege(access);
        let fault = Trap {
            cause: match access {
                access::FETCH => exception::INSTRUCTION_PAGE,
                access::LOAD => exception::LOAD_PAGE,
                _ => exception::STORE_PAGE,
            },
            value: va,
        };
        let afault = Trap {
            cause: match access {
                access::FETCH => exception::INSTRUCTION_ACCESS,
                access::LOAD => exception::LOAD_ACCESS,
                _ => exception::STORE_ACCESS,
            },
            value: va,
        };
        let satp = self.csrs[csr::SATP];
        let pa = if privilege == privilege::MACHINE || satp >> 60 == 0 {
            va
        } else {
            if sext(va, 39) != va {
                return Err(fault);
            }
            let mut table = (satp & PPN_MASK) << PAGE_SHIFT;
            let mut physical = None;
            for level in (0..3).rev() {
                let paddr = table + ((va >> (PAGE_SHIFT + VPN_BITS * level)) & VPN_MASK) * 8;
                if !self.pmp_check(paddr, 8, access::LOAD, privilege::SUPERVISOR) {
                    return Err(afault);
                }
                let pte = self.memory.load(paddr, 8).map_err(|_| afault)?;
                let pbmt = (pte >> 61) & 3;
                let reserved = (!0u64 << 54) & !(3u64 << 61);
                if pte & PTE_VALID == 0
                    || pte & (PTE_READ | PTE_WRITE) == PTE_WRITE
                    || pte & reserved != 0
                    || pbmt == 3
                    || pbmt != 0 && self.csrs[csr::MENVCFG] & (1 << 62) == 0
                {
                    return Err(fault);
                }
                let ppn = (pte >> 10) & PPN_MASK;
                if pte & (PTE_READ | PTE_EXECUTE) != 0 {
                    let user = pte & PTE_USER != 0;
                    if privilege == privilege::USER && !user
                        || privilege == privilege::SUPERVISOR
                            && user
                            && (access == access::FETCH
                                || self.csrs[csr::MSTATUS] & status::SUM == 0)
                    {
                        return Err(fault);
                    }
                    let allowed = match access {
                        access::FETCH => pte & PTE_EXECUTE != 0,
                        access::LOAD => {
                            pte & PTE_READ != 0
                                || pte & PTE_EXECUTE != 0
                                    && self.csrs[csr::MSTATUS] & status::MXR != 0
                        }
                        _ => pte & PTE_WRITE != 0,
                    };
                    let lowmask = (1u64 << (VPN_BITS * level)) - 1;
                    if !allowed || ppn & lowmask != 0 {
                        return Err(fault);
                    }
                    // Svade: missing A/D bits fault rather than modifying the PTE.
                    if pte & PTE_ACCESSED == 0 || access == access::STORE && pte & PTE_DIRTY == 0 {
                        return Err(fault);
                    }
                    physical = Some(
                        ((ppn & !lowmask) << PAGE_SHIFT)
                            | (va & ((1u64 << (PAGE_SHIFT + VPN_BITS * level)) - 1)),
                    );
                    break;
                }
                if pte & (PTE_USER | PTE_ACCESSED | PTE_DIRTY) != 0 || pbmt != 0 {
                    return Err(fault);
                }
                table = ppn << PAGE_SHIFT;
            }
            physical.ok_or(fault)?
        };
        if !self.pmp_check(pa, size, access, privilege) {
            return Err(afault);
        }
        Ok(pa)
    }

    #[inline]
    pub fn load_virtual(&mut self, va: u64, size: usize, access: u8) -> Result<u64, Trap> {
        if (va & PAGE_OFFSET_MASK) + size as u64 > PAGE_SIZE as u64 {
            return self.load_split_pages(va, size, access);
        }
        let pa = self.cached_translate(va, size, access)?;
        self.memory.load(pa, size).map_err(|_| Trap {
            cause: if access == access::FETCH {
                exception::INSTRUCTION_ACCESS
            } else {
                exception::LOAD_ACCESS
            },
            value: va,
        })
    }

    #[inline]
    pub fn store_virtual(&mut self, va: u64, size: usize, value: u64) -> Result<(), Trap> {
        if (va & PAGE_OFFSET_MASK) + size as u64 > PAGE_SIZE as u64 {
            return self.store_split_pages(va, size, value);
        }
        let pa = self.cached_translate(va, size, access::STORE)?;
        self.memory.store(pa, size, value).map_err(|_| Trap {
            cause: exception::STORE_ACCESS,
            value: va,
        })
    }

    #[cold]
    fn store_split_pages(&mut self, va: u64, size: usize, value: u64) -> Result<(), Trap> {
        let first = PAGE_SIZE - (va & PAGE_OFFSET_MASK) as usize;
        // Match Spike: validate and commit each page fragment in order. A
        // fault on the second page leaves the first page's bytes written.
        for (offset, len) in [(0, first), (first, size - first)] {
            let part = va.wrapping_add(offset as u64);
            let pa = self.translate(part, len, access::STORE)?;
            self.memory
                .store(pa, len, value >> (offset * 8))
                .map_err(|_| Trap {
                    cause: exception::STORE_ACCESS,
                    value: part,
                })?;
        }
        Ok(())
    }

    #[cold]
    fn load_split_pages(&mut self, va: u64, size: usize, access: u8) -> Result<u64, Trap> {
        let first = PAGE_SIZE - (va & PAGE_OFFSET_MASK) as usize;
        let mut value = 0;
        // PMP checks apply to each entire page fragment, not individual bytes.
        for (offset, len) in [(0, first), (first, size - first)] {
            let part = va.wrapping_add(offset as u64);
            let pa = self.cached_translate(part, len, access)?;
            let data = self.memory.load_fragment(pa, len).map_err(|_| Trap {
                cause: if access == access::FETCH {
                    exception::INSTRUCTION_ACCESS
                } else {
                    exception::LOAD_ACCESS
                },
                value: part,
            })?;
            value |= data << (offset * 8);
        }
        Ok(value)
    }
}
