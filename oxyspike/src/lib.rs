//! Safe-Rust RV64 interpreter with decoded-instruction and translation caches.
//!
//! `Cpu::step` executes one instruction; the host owns interrupt polling, timer
//! advancement and HTIF servicing. See the CLI for the single-hart platform loop.

pub mod arch;
mod bitmanip;
mod compressed;
pub mod csr;
mod decode_cache;
pub mod elf;
mod fast;
mod floating;
pub mod memory;
mod mmu;
mod privileged;
#[cfg(feature = "dispatch-profile")]
mod profile;
pub mod softfloat;
mod status;

use arch::{access, exception, privilege};
use memory::Memory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trap {
    pub cause: u64,
    pub value: u64,
}

#[inline(always)]
fn sext(v: u64, bits: u32) -> u64 {
    ((v << (64 - bits)) as i64 >> (64 - bits)) as u64
}

/// Architectural state and caches for one RV64 hart.
///
/// Host writes to memory require `flush_instruction` before executing modified
/// code, and `flush_translation` after changing page tables. Direct CSR or device
/// changes also require `memory.interrupt_dirty` before `poll_interrupt`.
pub struct Cpu {
    pub x: [u64; 32],
    pub f: [u64; 32],
    pub pc: u64,
    pub retired: u64,
    pub memory: Memory,
    reservation: Option<(u64, usize)>,
    pub privilege: u8,
    pub csrs: [u64; 4096],
    pub cycles: u64,
    counter_written: u8,
    tlb: [Box<[mmu::TlbEntry]>; 3],
    decoded: Box<[decode_cache::Entry; decode_cache::ENTRIES]>,
    decode_epoch: u64,
    decode_satp: u64,
    #[cfg(feature = "dispatch-profile")]
    profile: profile::DispatchProfile,
}

const COUNTER_CYCLE: u8 = 1;
const COUNTER_INSTRET: u8 = 4;

impl Cpu {
    pub fn new(memory: Memory, pc: u64) -> Self {
        let pending = if memory.msip & 1 != 0 { 8 } else { 0 }
            | if memory.mtime >= memory.mtimecmp {
                128
            } else {
                0
            };
        Self {
            x: [0; 32],
            f: [0; 32],
            pc,
            retired: 0,
            memory,
            reservation: None,
            privilege: privilege::MACHINE,
            csrs: {
                let mut c = [0; 4096];
                c[csr::MSTATUS] = (2 << 32) | (2 << 34);
                c[csr::MIP] = pending;
                c
            },
            cycles: 0,
            counter_written: 0,
            decoded: decode_cache::new(),
            decode_epoch: 1,
            decode_satp: 0,
            #[cfg(feature = "dispatch-profile")]
            profile: profile::DispatchProfile::default(),
            tlb: std::array::from_fn(|_| {
                vec![mmu::TlbEntry::EMPTY; mmu::TLB_ENTRIES].into_boxed_slice()
            }),
        }
    }

    #[inline(always)]
    pub fn step(&mut self) -> Result<(), Trap> {
        let pc = self.pc;
        if pc & 1 != 0 {
            return Err(Trap {
                cause: exception::INSTRUCTION_MISALIGNED,
                value: pc,
            });
        }
        // Only CY and IR affect the implemented running counters.
        let inhibited = self.csrs[csr::MCOUNTINHIBIT] as u8 & (COUNTER_CYCLE | COUNTER_INSTRET);
        self.execute_cached()?;
        self.x[0] = 0;
        let suppressed = inhibited | self.counter_written;
        if suppressed == 0 {
            self.retired = self.retired.wrapping_add(1);
            self.cycles = self.cycles.wrapping_add(1);
        } else {
            self.counter_written = 0;
            if suppressed & COUNTER_INSTRET == 0 {
                self.retired = self.retired.wrapping_add(1);
            }
            if suppressed & COUNTER_CYCLE == 0 {
                self.cycles = self.cycles.wrapping_add(1);
            }
        }
        Ok(())
    }

    #[inline(never)]
    fn execute(&mut self, i: u32, len: u64) -> Result<(), Trap> {
        let rd = ((i >> 7) & 31) as usize;
        let rs1 = ((i >> 15) & 31) as usize;
        let rs2 = ((i >> 20) & 31) as usize;
        let a = self.x[rs1];
        let b = self.x[rs2];
        let f = (i >> 12) & 7;
        let hi = i >> 25;
        let imm = sext((i >> 20) as u64, 12);
        let illegal = Trap {
            cause: exception::ILLEGAL_INSTRUCTION,
            value: i as u64,
        };
        let mut next = self.pc.wrapping_add(len);
        let result = match i & 127 {
            0x37 => Some(sext((i & 0xfffff000) as u64, 32)),
            0x17 => Some(self.pc.wrapping_add(sext((i & 0xfffff000) as u64, 32))),
            0x6f => {
                let off = ((i >> 31) << 20)
                    | (((i >> 12) & 255) << 12)
                    | (((i >> 20) & 1) << 11)
                    | (((i >> 21) & 1023) << 1);
                let ret = next;
                next = self.pc.wrapping_add(sext(off as u64, 21));
                Some(ret)
            }
            0x67 if f == 0 => {
                let ret = next;
                next = a.wrapping_add(imm) & !1;
                Some(ret)
            }
            0x63 => {
                let take = match f {
                    0 => a == b,
                    1 => a != b,
                    4 => (a as i64) < (b as i64),
                    5 => (a as i64) >= (b as i64),
                    6 => a < b,
                    7 => a >= b,
                    _ => return Err(illegal),
                };
                if take {
                    let off = ((i >> 31) << 12)
                        | (((i >> 7) & 1) << 11)
                        | (((i >> 25) & 63) << 5)
                        | (((i >> 8) & 15) << 1);
                    next = self.pc.wrapping_add(sext(off as u64, 13));
                }
                None
            }
            0x07 if (1..=3).contains(&f) => {
                if self.csrs[csr::MSTATUS] & status::FS == 0 {
                    return Err(illegal);
                }
                let value = self.load_virtual(a.wrapping_add(imm), 1 << f, access::LOAD)?;
                self.f[rd] = if f == 1 {
                    value | 0xffffffffffff0000
                } else if f == 2 {
                    value | 0xffffffff00000000
                } else {
                    value
                };
                self.csrs[csr::MSTATUS] |= status::FS;
                None
            }
            0x27 if (1..=3).contains(&f) => {
                if self.csrs[csr::MSTATUS] & status::FS == 0 {
                    return Err(illegal);
                }
                let off = sext((((i >> 25) << 5) | ((i >> 7) & 31)) as u64, 12);
                self.store_virtual(a.wrapping_add(off), 1 << f, self.f[rs2])?;
                None
            }
            0x43 | 0x47 | 0x4b | 0x4f | 0x53 => self.floating(i)?,
            0x03 => {
                let size = match f {
                    0 | 4 => 1,
                    1 | 5 => 2,
                    2 | 6 => 4,
                    3 => 8,
                    _ => return Err(illegal),
                };
                let v = self.load_virtual(a.wrapping_add(imm), size, access::LOAD)?;
                Some(if f < 3 { sext(v, (size * 8) as u32) } else { v })
            }
            0x23 => {
                let off = sext((((i >> 25) << 5) | ((i >> 7) & 31)) as u64, 12);
                let size = match f {
                    0 => 1,
                    1 => 2,
                    2 => 4,
                    3 => 8,
                    _ => return Err(illegal),
                };
                self.store_virtual(a.wrapping_add(off), size, b)?;
                self.reservation = None;
                None
            }
            0x13 | 0x1b | 0x33 | 0x3b if bitmanip::execute(i, a, b).is_some() => {
                bitmanip::execute(i, a, b)
            }
            0x13 => Some(match f {
                0 => a.wrapping_add(imm),
                2 => ((a as i64) < (imm as i64)) as u64,
                3 => (a < imm) as u64,
                4 => a ^ imm,
                6 => a | imm,
                7 => a & imm,
                1 if i >> 26 == 0 => a << ((i >> 20) & 63),
                5 if i >> 26 == 0 => a >> ((i >> 20) & 63),
                5 if i >> 26 == 16 => ((a as i64) >> ((i >> 20) & 63)) as u64,
                _ => return Err(illegal),
            }),
            0x1b => Some(sext(
                match f {
                    0 => a.wrapping_add(imm),
                    1 if hi == 0 => a << ((i >> 20) & 31),
                    5 if hi == 0 => (a as u32 >> ((i >> 20) & 31)) as u64,
                    5 if hi == 32 => ((a as i32) >> ((i >> 20) & 31)) as u64,
                    _ => return Err(illegal),
                },
                32,
            )),
            0x33 | 0x3b => {
                let word = i & 127 == 0x3b;
                let v = if hi == 1 {
                    self.multiply(a, b, f, word).ok_or(illegal)?
                } else {
                    let sh = b & if word { 31 } else { 63 };
                    match (f, hi) {
                        (0, 0) => a.wrapping_add(b),
                        (0, 32) => a.wrapping_sub(b),
                        (1, 0) => a << sh,
                        (2, 0) if !word => ((a as i64) < (b as i64)) as u64,
                        (3, 0) if !word => (a < b) as u64,
                        (4, 0) if !word => a ^ b,
                        (6, 0) if !word => a | b,
                        (7, 0) if !word => a & b,
                        (5, 0) => {
                            if word {
                                (a as u32 >> sh) as u64
                            } else {
                                a >> sh
                            }
                        }
                        (5, 32) => {
                            if word {
                                ((a as i32) >> sh) as u64
                            } else {
                                ((a as i64) >> sh) as u64
                            }
                        }
                        _ => return Err(illegal),
                    }
                };
                Some(if word { sext(v, 32) } else { v })
            }
            0x0f if f == 0 => None,
            0x0f if f == 1 => {
                self.flush_instruction();
                None
            }
            0x0f if f == 2 && rd == 0 && matches!(i >> 20, 0 | 1 | 2 | 4) => {
                let op = i >> 20;
                let mask = if op == 4 {
                    1 << 7
                } else if op == 0 {
                    3 << 4
                } else {
                    1 << 6
                };
                if self.privilege < privilege::MACHINE && self.csrs[csr::MENVCFG] & mask == 0
                    || self.privilege == privilege::USER && self.csrs[csr::SENVCFG] & mask == 0
                {
                    return Err(illegal);
                }
                // Spike translates the addressed byte, then operates on its RAM block.
                // Clean/invalidate uses load permissions but reports store faults.
                let access = if op == 4 { access::STORE } else { access::LOAD };
                let pa = self.translate(a, 1, access).map_err(|mut trap| {
                    if access == access::LOAD {
                        trap.cause += 2;
                    }
                    trap
                })? & !63;
                let fault = Trap {
                    cause: exception::STORE_ACCESS,
                    value: a,
                };
                if !self.memory.reservable(pa, 64) {
                    return Err(fault);
                }
                if op == 4 {
                    self.memory.zero_range(pa, 64).map_err(|_| fault)?;
                }
                None
            }
            0x2f if f == 2 || f == 3 => {
                let size = 1 << f;
                let op = i >> 27;
                // Reject malformed encodings before address checks or memory access.
                if (op == 2 && rs2 != 0)
                    || !matches!(op, 0 | 1 | 2 | 3 | 4 | 8 | 12 | 16 | 20 | 24 | 28)
                {
                    return Err(illegal);
                }
                if a & (size as u64 - 1) != 0 {
                    return Err(Trap {
                        cause: if op == 2 {
                            exception::LOAD_MISALIGNED
                        } else {
                            exception::STORE_MISALIGNED
                        },
                        value: a,
                    });
                }
                let pa =
                    self.translate(a, size, if op == 2 { access::LOAD } else { access::STORE })?;
                if matches!(op, 2 | 3) && !self.memory.reservable(pa, size) {
                    return Err(Trap {
                        cause: if op == 2 {
                            exception::LOAD_ACCESS
                        } else {
                            exception::STORE_ACCESS
                        },
                        value: a,
                    });
                }
                if op == 3 {
                    // Validate the address even when the reservation fails.
                    self.memory.load(pa, size).map_err(|_| Trap {
                        cause: exception::STORE_ACCESS,
                        value: a,
                    })?;
                    let success = self.reservation == Some((pa, size));
                    self.reservation = None;
                    if success {
                        self.memory.store(pa, size, b)?;
                    }
                    Some((!success) as u64)
                } else {
                    let old = self.memory.load(pa, size).map_err(|_| Trap {
                        cause: if op == 2 {
                            exception::LOAD_ACCESS
                        } else {
                            exception::STORE_ACCESS
                        },
                        value: a,
                    })?;
                    let old = if size == 4 { sext(old, 32) } else { old };
                    let rhs = if size == 4 { sext(b, 32) } else { b };
                    if op == 2 {
                        self.reservation = Some((pa, size));
                    } else {
                        let new = match op {
                            0 => old.wrapping_add(rhs),
                            1 => rhs,
                            4 => old ^ rhs,
                            8 => old | rhs,
                            12 => old & rhs,
                            16 => {
                                if (old as i64) < (rhs as i64) {
                                    old
                                } else {
                                    rhs
                                }
                            }
                            20 => {
                                if (old as i64) > (rhs as i64) {
                                    old
                                } else {
                                    rhs
                                }
                            }
                            24 => old.min(rhs),
                            28 => old.max(rhs),
                            _ => unreachable!(),
                        };
                        self.memory.store(pa, size, new)?;
                        self.reservation = None;
                    }
                    Some(old)
                }
            }
            0x73 => self.system(i, &mut next)?,
            _ => return Err(illegal),
        };
        if let Some(v) = result
            && rd != 0
        {
            self.x[rd] = v;
        }
        self.pc = next;
        Ok(())
    }

    #[inline(always)]
    fn multiply(&self, a: u64, b: u64, f: u32, word: bool) -> Option<u64> {
        let (a, b) = if word {
            (sext(a, 32), sext(b, 32))
        } else {
            (a, b)
        };
        let (ua, ub) = if word {
            (a as u32 as u64, b as u32 as u64)
        } else {
            (a, b)
        };
        Some(match f {
            0 => a.wrapping_mul(b),
            1 if !word => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
            2 if !word => (((a as i64 as i128) * (b as i128)) >> 64) as u64,
            3 if !word => (((a as u128) * (b as u128)) >> 64) as u64,
            4 => {
                if b == 0 {
                    u64::MAX
                } else {
                    (a as i64).wrapping_div(b as i64) as u64
                }
            }
            5 => ua.checked_div(ub).unwrap_or(u64::MAX),
            6 => {
                if b == 0 {
                    a
                } else {
                    (a as i64).wrapping_rem(b as i64) as u64
                }
            }
            7 => {
                if ub == 0 {
                    ua
                } else {
                    ua % ub
                }
            }
            _ => return None,
        })
    }
}
