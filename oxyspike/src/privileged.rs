use crate::arch::{exception, privilege};
use crate::{Cpu, Trap, csr, status};
pub const SSTATUS_MASK: u64 = status::SIE
    | status::SPIE
    | status::SPP
    | status::FS
    | status::XS
    | status::SUM
    | status::MXR
    | status::UXL
    | status::SD;
const STATUS_WRITE: u64 = status::SIE
    | status::MIE
    | status::SPIE
    | status::MPIE
    | status::SPP
    | status::MPP
    | status::FS
    | status::MPRV
    | status::SUM
    | status::MXR
    | status::TVM
    | status::TW
    | status::TSR;
impl Cpu {
    pub fn csr_read(&self, csr: usize) -> Option<u64> {
        let s = self.csrs[csr::MSTATUS];
        Some(match csr {
            csr::FFLAGS => self.csrs[csr::FCSR] & 31,
            csr::FRM => (self.csrs[csr::FCSR] >> 5) & 7,
            csr::FCSR => self.csrs[csr::FCSR],
            csr::SSTATUS => self.csr_read(csr::MSTATUS)? & SSTATUS_MASK,
            csr::MSTATUS => s | if (s >> 13) & 3 == 3 { 1 << 63 } else { 0 },
            // RV64 with A, C, D, F, I, M, S and U extensions.
            csr::MISA => (2 << 62) | 0x14_112d,
            csr::SIE => self.csrs[csr::MIE] & self.csrs[csr::MIDELEG],
            csr::SIP => self.csrs[csr::MIP] & self.csrs[csr::MIDELEG],
            csr::MCYCLE | csr::CYCLE => self.cycles,
            csr::TIME => self.memory.mtime,
            csr::MINSTRET | csr::INSTRET => self.retired,
            csr::MVENDORID..=csr::MHARTID
            | csr::MHPMCOUNTER3..=csr::MHPMCOUNTER31
            | csr::HPMCOUNTER3..=csr::HPMCOUNTER31
            | csr::MHPMEVENT3..=csr::MHPMEVENT31 => 0,
            csr::SENVCFG
            | csr::MENVCFG
            | csr::STVEC
            | csr::SCOUNTEREN
            | csr::SSCRATCH..=csr::STVAL
            | csr::SATP
            | csr::MEDELEG..=csr::MCOUNTEREN
            | csr::MCOUNTINHIBIT
            | csr::MSCRATCH..=csr::MIP
            | csr::PMPCFG0
            | csr::PMPCFG2
            | csr::PMPADDR0..=csr::PMPADDR15 => self.csrs[csr],
            _ => return None,
        })
    }

    pub fn csr_write(&mut self, csr: usize, val: u64) {
        self.memory.interrupt_dirty = true;
        if matches!(
            csr,
            csr::PMPCFG0 | csr::PMPCFG2 | csr::PMPADDR0..=csr::PMPADDR15
        ) {
            self.flush_translation();
        }
        match csr {
            csr::SENVCFG | csr::MENVCFG => {
                let mask = 0xf1 | if csr == csr::MENVCFG { 1u64 << 62 } else { 0 };
                let mut value = val & mask;
                if value & 0x30 == 0x20 {
                    value &= !0x30;
                }
                if csr == csr::MENVCFG && (self.csrs[csr] ^ value) & (1 << 62) != 0 {
                    self.flush_translation();
                }
                self.csrs[csr] = value;
            }
            csr::FFLAGS => {
                self.csrs[csr::FCSR] = (self.csrs[csr::FCSR] & !31) | (val & 31);
                self.csrs[csr::MSTATUS] |= status::FS;
            }
            csr::FRM => {
                self.csrs[csr::FCSR] = (self.csrs[csr::FCSR] & 31) | ((val & 7) << 5);
                self.csrs[csr::MSTATUS] |= status::FS;
            }
            csr::FCSR => {
                self.csrs[csr::FCSR] = val & 255;
                self.csrs[csr::MSTATUS] |= status::FS;
            }
            csr::SSTATUS => self.csr_write(
                csr::MSTATUS,
                (self.csrs[csr::MSTATUS] & !SSTATUS_MASK) | (val & SSTATUS_MASK),
            ),
            csr::MSTATUS => {
                let mut v = (self.csrs[csr] & !STATUS_WRITE) | (val & STATUS_WRITE);
                if (v >> 11) & 3 == 2 {
                    v &= !status::MPP;
                }
                self.csrs[csr] = v;
            }
            csr::MISA => {} // Fixed supported ISA (legal WARL choice).
            csr::SIE => {
                self.csrs[csr::MIE] = (self.csrs[csr::MIE] & !self.csrs[csr::MIDELEG])
                    | (val & self.csrs[csr::MIDELEG] & 0x222)
            }
            csr::SIP => {
                self.csrs[csr::MIP] =
                    (self.csrs[csr::MIP] & !2) | (val & self.csrs[csr::MIDELEG] & 2)
            }
            csr::MIP => self.csrs[csr] = (self.csrs[csr] & !0x222) | (val & 0x222),
            csr::MIE => self.csrs[csr] = val & 0xaaa,
            csr::MEDELEG => self.csrs[csr] = val & 0xb3ff,
            csr::MIDELEG => self.csrs[csr] = val & 0x222,
            csr::MTVEC | csr::STVEC => self.csrs[csr] = (val & !3) | u64::from(val & 3 == 1),
            csr::MEPC | csr::SEPC => self.csrs[csr] = val & !1,
            csr::SATP => {
                if matches!(val >> 60, 0 | 8) {
                    self.csrs[csr] = val;
                }
            }
            csr::MCOUNTEREN | csr::SCOUNTEREN => self.csrs[csr] = val & 0xffff_ffff,
            csr::MCOUNTINHIBIT => self.csrs[csr] = val & 0xffff_fffd,
            csr::MHPMCOUNTER3..=csr::MHPMCOUNTER31 | csr::MHPMEVENT3..=csr::MHPMEVENT31 => {} // No hardware performance events.
            csr::MCYCLE => {
                self.cycles = val;
            }
            csr::MINSTRET => {
                self.retired = val;
            }
            csr::PMPCFG0 | csr::PMPCFG2 => {
                for n in 0..8 {
                    let old = (self.csrs[csr] >> (n * 8)) & 255;
                    if old & 128 != 0 {
                        continue;
                    }
                    let mut cfg = (val >> (n * 8)) & 0x9f;
                    if cfg & 3 == 2 {
                        cfg &= !2;
                    }
                    self.csrs[csr] = (self.csrs[csr] & !(255 << (n * 8))) | (cfg << (n * 8));
                }
            }
            csr::PMPADDR0..=csr::PMPADDR15 => {
                let n = csr - csr::PMPADDR0;
                let cfg = self.pmp_cfg(n);
                let next = if n < 15 { self.pmp_cfg(n + 1) } else { 0 };
                if cfg & 128 == 0 && next & 0x98 != 0x88 {
                    self.csrs[csr] = val & ((1u64 << 54) - 1);
                }
            }
            _ => self.csrs[csr] = val,
        }
    }
    pub(crate) fn pmp_cfg(&self, n: usize) -> u64 {
        (self.csrs[csr::PMPCFG0 + (n / 8) * 2] >> ((n % 8) * 8)) & 255
    }

    pub fn take_trap(&mut self, trap: Trap) {
        self.memory.interrupt_dirty = true;
        let interrupt = trap.cause >> 63 != 0;
        let code = trap.cause & 63;
        let deleg = self.csrs[if interrupt {
            csr::MIDELEG
        } else {
            csr::MEDELEG
        }];
        let supervisor = self.privilege < privilege::MACHINE && deleg & (1 << code) != 0;
        let s = self.csrs[csr::MSTATUS];
        let base = if supervisor {
            self.csrs[csr::SEPC] = self.pc;
            self.csrs[csr::SCAUSE] = trap.cause;
            self.csrs[csr::STVAL] = trap.value;
            self.csrs[csr::MSTATUS] = (s & !(status::SIE | status::SPIE | status::SPP))
                | ((s & status::SIE) << 4)
                | ((self.privilege as u64) << 8);
            self.privilege = privilege::SUPERVISOR;
            self.csrs[csr::STVEC]
        } else {
            self.csrs[csr::MEPC] = self.pc;
            self.csrs[csr::MCAUSE] = trap.cause;
            self.csrs[csr::MTVAL] = trap.value;
            self.csrs[csr::MSTATUS] = (s & !(status::MIE | status::MPIE | status::MPP))
                | ((s & status::MIE) << 4)
                | ((self.privilege as u64) << 11);
            self.privilege = privilege::MACHINE;
            self.csrs[csr::MTVEC]
        };
        self.pc = (base & !3)
            + if interrupt && base & 3 == 1 {
                4 * code
            } else {
                0
            };
        self.reservation = None;
    }

    /// Event-driven polling for the CLI. Direct host mutations of public CSR,
    /// privilege or CLINT fields must set `memory.interrupt_dirty` first.
    #[inline(always)]
    pub fn poll_interrupt(&mut self) -> bool {
        if !self.memory.interrupt_dirty {
            return false;
        }
        self.memory.interrupt_dirty = false;
        self.interrupt()
    }

    pub fn interrupt(&mut self) -> bool {
        self.csrs[csr::MIP] = (self.csrs[csr::MIP] & !0x88)
            | if self.memory.msip & 1 != 0 { 8 } else { 0 }
            | if self.memory.mtime >= self.memory.mtimecmp {
                128
            } else {
                0
            };
        let pending = self.csrs[csr::MIP] & self.csrs[csr::MIE];
        if pending == 0 {
            return false;
        }
        for n in [11, 3, 7, 9, 1, 5] {
            if pending & (1 << n) == 0 {
                continue;
            }
            let delegated = self.csrs[csr::MIDELEG] & (1 << n) != 0;
            let enabled = if delegated {
                self.privilege == privilege::USER
                    || self.privilege == privilege::SUPERVISOR
                        && self.csrs[csr::MSTATUS] & status::SIE != 0
            } else {
                self.privilege < privilege::MACHINE || self.csrs[csr::MSTATUS] & status::MIE != 0
            };
            if enabled {
                self.take_trap(Trap {
                    cause: (1 << 63) | n,
                    value: 0,
                });
                return true;
            }
        }
        false
    }
    pub(crate) fn system(&mut self, i: u32, next: &mut u64) -> Result<Option<u64>, Trap> {
        let illegal = Trap {
            cause: exception::ILLEGAL_INSTRUCTION,
            value: i as u64,
        };
        let f = (i >> 12) & 7;
        let rs = ((i >> 15) & 31) as usize;
        let csr = (i >> 20) as usize;
        if f != 0 {
            if f == 4 {
                return Err(illegal);
            }
            let val = if f & 4 != 0 { rs as u64 } else { self.x[rs] };
            let write = f & 3 == 1 || rs != 0;
            if self.privilege < ((csr >> 8) & 3) as u8 || write && csr >> 10 == 3 {
                return Err(illegal);
            }
            if csr == csr::SATP
                && self.privilege == privilege::SUPERVISOR
                && self.csrs[csr::MSTATUS] & status::TVM != 0
            {
                return Err(illegal);
            }
            if (csr::CYCLE..=csr::HPMCOUNTER31).contains(&csr)
                && self.privilege < privilege::MACHINE
            {
                let bit = 1 << (csr - csr::CYCLE);
                if self.csrs[csr::MCOUNTEREN] & bit == 0
                    || self.privilege == privilege::USER && self.csrs[csr::SCOUNTEREN] & bit == 0
                {
                    return Err(illegal);
                }
            }
            if (csr::FFLAGS..=csr::FCSR).contains(&csr) && self.csrs[csr::MSTATUS] & status::FS == 0
            {
                return Err(illegal);
            }
            let old = self.csr_read(csr).ok_or(illegal)?;
            if write {
                self.csr_write(
                    csr,
                    match f & 3 {
                        1 => val,
                        2 => old | val,
                        3 => old & !val,
                        _ => unreachable!(),
                    },
                );
            }
            if write && csr == csr::MCYCLE {
                self.counter_written |= crate::COUNTER_CYCLE;
            }
            if write && csr == csr::MINSTRET {
                self.counter_written |= crate::COUNTER_INSTRET;
            }
            return Ok(Some(old));
        }
        match i {
            0x73 => {
                return Err(Trap {
                    cause: exception::ECALL_USER + self.privilege as u64,
                    value: 0,
                });
            }
            0x00100073 => {
                return Err(Trap {
                    cause: exception::BREAKPOINT,
                    value: self.pc,
                });
            }
            0x30200073 if self.privilege == privilege::MACHINE => {
                self.memory.interrupt_dirty = true;
                let s = self.csrs[csr::MSTATUS];
                self.privilege = ((s >> 11) & 3) as u8;
                self.csrs[csr::MSTATUS] = (s & !(status::MIE | status::MPIE | status::MPP))
                    | ((s & status::MPIE) >> 4)
                    | status::MPIE;
                if self.privilege != privilege::MACHINE {
                    self.csrs[csr::MSTATUS] &= !status::MPRV;
                }
                *next = self.csrs[csr::MEPC];
            }
            0x10200073
                if self.privilege >= privilege::SUPERVISOR
                    && (self.privilege == privilege::MACHINE
                        || self.csrs[csr::MSTATUS] & status::TSR == 0) =>
            {
                self.memory.interrupt_dirty = true;
                let s = self.csrs[csr::MSTATUS];
                self.privilege = ((s >> 8) & 1) as u8;
                self.csrs[csr::MSTATUS] =
                    (s & !(status::SIE | status::SPIE | status::SPP | status::MPRV))
                        | ((s & status::SPIE) >> 4)
                        | status::SPIE;
                *next = self.csrs[csr::SEPC];
            }
            0x10500073
                if self.privilege >= privilege::SUPERVISOR
                    && (self.privilege == privilege::MACHINE
                        || self.csrs[csr::MSTATUS] & status::TW == 0) =>
            {
                self.memory.interrupt_dirty = true;
            }
            _ if matches!(i & 0xfe007fff, 0x12000073 | 0x16000073)
                && self.privilege >= privilege::SUPERVISOR
                && (self.privilege == privilege::MACHINE
                    || self.csrs[csr::MSTATUS] & status::TVM == 0) =>
            {
                self.flush_translation();
            }
            0x18000073 | 0x18100073 if self.privilege >= privilege::SUPERVISOR => {}
            _ => return Err(illegal),
        }
        Ok(None)
    }
}
