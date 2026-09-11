//! Decode common integer operations once, keeping bit extraction off the hot path.
use crate::{Cpu, Trap, arch::access, sext};
#[derive(Clone, Copy, Default)]
#[repr(u8)]
pub(crate) enum Op {
    #[default]
    Slow,
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Ld,
    Lbu,
    Lhu,
    Lwu,
    Sb,
    Sh,
    Sw,
    Sd,
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
    Addiw,
    Slliw,
    Srliw,
    Sraiw,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Addw,
    Subw,
    Sllw,
    Srlw,
    Sraw,
    Mulw,
    Sh1addUw,
    Sh2addUw,
    ZextH,
}

#[derive(Clone, Copy, Default)]
// Keep the operation byte first and the decoded payload eight bytes: this
// layout is deliberate and benchmarked on the dispatch hot path.
#[repr(C)]
pub(crate) struct Decoded {
    op: Op,
    rd: u8,
    rs1: u8,
    rs2: u8,
    // Immediate for fast operations; full expanded instruction for Slow.
    imm: i32,
}

const _: () = assert!(std::mem::size_of::<Decoded>() == 8);
impl Decoded {
    pub fn new(i: u32) -> Self {
        use Op::*;
        let mut d = Self {
            op: Slow,
            rd: ((i >> 7) & 31) as u8,
            rs1: ((i >> 15) & 31) as u8,
            rs2: ((i >> 20) & 31) as u8,
            imm: sext((i >> 20) as u64, 12) as i32,
        };
        let f = (i >> 12) & 7;
        let hi = i >> 25;
        d.op = match i & 127 {
            0x37 | 0x17 => {
                d.imm = (i & 0xfffff000) as i32;
                if i & 127 == 0x37 { Lui } else { Auipc }
            }
            0x6f => {
                d.imm = sext(
                    (((i >> 31) << 20)
                        | (((i >> 12) & 255) << 12)
                        | (((i >> 20) & 1) << 11)
                        | (((i >> 21) & 1023) << 1)) as u64,
                    21,
                ) as i32;
                Jal
            }
            0x67 if f == 0 => Jalr,
            0x63 => {
                d.imm = sext(
                    (((i >> 31) << 12)
                        | (((i >> 7) & 1) << 11)
                        | (((i >> 25) & 63) << 5)
                        | (((i >> 8) & 15) << 1)) as u64,
                    13,
                ) as i32;
                match f {
                    0 => Beq,
                    1 => Bne,
                    4 => Blt,
                    5 => Bge,
                    6 => Bltu,
                    7 => Bgeu,
                    _ => Slow,
                }
            }
            0x03 => match f {
                0 => Lb,
                1 => Lh,
                2 => Lw,
                3 => Ld,
                4 => Lbu,
                5 => Lhu,
                6 => Lwu,
                _ => Slow,
            },
            0x23 => {
                d.imm = sext((((i >> 25) << 5) | ((i >> 7) & 31)) as u64, 12) as i32;
                match f {
                    0 => Sb,
                    1 => Sh,
                    2 => Sw,
                    3 => Sd,
                    _ => Slow,
                }
            }
            0x13 => match f {
                0 => Addi,
                2 => Slti,
                3 => Sltiu,
                4 => Xori,
                6 => Ori,
                7 => Andi,
                1 if i >> 26 == 0 => Slli,
                5 if i >> 26 == 0 => Srli,
                5 if i >> 26 == 16 => Srai,
                _ => Slow,
            },
            0x1b => match (f, hi) {
                (0, _) => Addiw,
                (1, 0) => Slliw,
                (5, 0) => Srliw,
                (5, 32) => Sraiw,
                _ => Slow,
            },
            0x33 => match (f, hi) {
                (0, 0) => Add,
                (0, 32) => Sub,
                (1, 0) => Sll,
                (2, 0) => Slt,
                (3, 0) => Sltu,
                (4, 0) => Xor,
                (5, 0) => Srl,
                (5, 32) => Sra,
                (6, 0) => Or,
                (7, 0) => And,
                (0, 1) => Mul,
                _ => Slow,
            },
            0x3b => match (f, hi) {
                (0, 0) => Addw,
                (0, 32) => Subw,
                (1, 0) => Sllw,
                (5, 0) => Srlw,
                (5, 32) => Sraw,
                (0, 1) => Mulw,
                (2, 0x10) => Sh1addUw,
                (4, 0x10) => Sh2addUw,
                (4, 4) if d.rs2 == 0 => ZextH,
                _ => Slow,
            },
            _ => Slow,
        };
        if matches!(d.op, Op::Slow) {
            d.imm = i as i32;
        }
        d
    }
}

impl Cpu {
    #[inline(always)]
    pub(crate) fn execute_decoded(&mut self, d: Decoded, len: u64) -> Result<(), Trap> {
        use Op::*;
        #[cfg(feature = "dispatch-profile")]
        {
            self.profile.operations[d.op as usize] += 1;
            if matches!(d.op, Slow) {
                let insn = d.imm as u32;
                self.profile.slow_opcodes[(insn & 127) as usize] += 1;
                let group = match insn & 127 {
                    0x13 => Some(0),
                    0x1b => Some(1),
                    0x33 => Some(2),
                    0x3b => Some(3),
                    _ => None,
                };
                if let Some(group) = group {
                    let function = ((insn >> 25) << 3) | ((insn >> 12) & 7);
                    self.profile.slow_functions[group * 1024 + function as usize] += 1;
                }
            }
        }
        let a = self.x[(d.rs1 & 31) as usize];
        let b = self.x[(d.rs2 & 31) as usize];
        let k = d.imm as u64;
        let mut next = self.pc.wrapping_add(len);
        let value = match d.op {
            Slow => return self.execute(d.imm as u32, len),
            Lui => k,
            Auipc => self.pc.wrapping_add(k),
            Jal => {
                let ret = next;
                next = self.pc.wrapping_add(k);
                ret
            }
            Jalr => {
                let ret = next;
                next = a.wrapping_add(k) & !1;
                ret
            }
            Beq => {
                self.pc = if a == b {
                    self.pc.wrapping_add(k)
                } else {
                    next
                };
                return Ok(());
            }
            Bne => {
                self.pc = if a != b {
                    self.pc.wrapping_add(k)
                } else {
                    next
                };
                return Ok(());
            }
            Blt => {
                self.pc = if (a as i64) < (b as i64) {
                    self.pc.wrapping_add(k)
                } else {
                    next
                };
                return Ok(());
            }
            Bge => {
                self.pc = if (a as i64) >= (b as i64) {
                    self.pc.wrapping_add(k)
                } else {
                    next
                };
                return Ok(());
            }
            Bltu => {
                self.pc = if a < b { self.pc.wrapping_add(k) } else { next };
                return Ok(());
            }
            Bgeu => {
                self.pc = if a >= b {
                    self.pc.wrapping_add(k)
                } else {
                    next
                };
                return Ok(());
            }
            Lb => self.load_virtual(a.wrapping_add(k), 1, access::LOAD)? as i8 as u64,
            Lh => self.load_virtual(a.wrapping_add(k), 2, access::LOAD)? as i16 as u64,
            Lw => self.load_virtual(a.wrapping_add(k), 4, access::LOAD)? as i32 as u64,
            Ld => self.load_virtual(a.wrapping_add(k), 8, access::LOAD)?,
            Lbu => self.load_virtual(a.wrapping_add(k), 1, access::LOAD)?,
            Lhu => self.load_virtual(a.wrapping_add(k), 2, access::LOAD)?,
            Lwu => self.load_virtual(a.wrapping_add(k), 4, access::LOAD)?,
            Sb | Sh | Sw | Sd => {
                let size = match d.op {
                    Sb => 1,
                    Sh => 2,
                    Sw => 4,
                    _ => 8,
                };
                self.store_virtual(a.wrapping_add(k), size, b)?;
                self.reservation = None;
                self.pc = next;
                return Ok(());
            }
            Addi => a.wrapping_add(k),
            Slti => ((a as i64) < (k as i64)) as u64,
            Sltiu => (a < k) as u64,
            Xori => a ^ k,
            Ori => a | k,
            Andi => a & k,
            Slli => a << (k & 63),
            Srli => a >> (k & 63),
            Srai => ((a as i64) >> (k & 63)) as u64,
            Addiw => a.wrapping_add(k) as i32 as u64,
            Slliw => (a << (k & 31)) as i32 as u64,
            Srliw => ((a as u32) >> (k & 31)) as i32 as u64,
            Sraiw => ((a as i32) >> (k & 31)) as u64,
            Add => a.wrapping_add(b),
            Sub => a.wrapping_sub(b),
            Sll => a << (b & 63),
            Slt => ((a as i64) < (b as i64)) as u64,
            Sltu => (a < b) as u64,
            Xor => a ^ b,
            Srl => a >> (b & 63),
            Sra => ((a as i64) >> (b & 63)) as u64,
            Or => a | b,
            And => a & b,
            Mul => a.wrapping_mul(b),
            Addw => a.wrapping_add(b) as i32 as u64,
            Subw => a.wrapping_sub(b) as i32 as u64,
            Sllw => (a << (b & 31)) as i32 as u64,
            Srlw => ((a as u32) >> (b & 31)) as i32 as u64,
            Sraw => ((a as i32) >> (b & 31)) as u64,
            Mulw => a.wrapping_mul(b) as i32 as u64,
            Sh1addUw => ((a as u32 as u64) << 1).wrapping_add(b),
            Sh2addUw => ((a as u32 as u64) << 2).wrapping_add(b),
            ZextH => a as u16 as u64,
        };
        if d.rd != 0 {
            self.x[(d.rd & 31) as usize] = value;
        }
        self.pc = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, RAM_BASE};
    #[test]
    fn predecoded_unsigned_word_ops_preserve_full_width_results_and_aliases() {
        let mut fast = Cpu::new(Memory::new(4096), RAM_BASE);
        let mut slow = Cpu::new(Memory::new(4096), RAM_BASE);
        let values = [
            0,
            1,
            0xffff,
            0x80000000,
            0xffffffff,
            0x1234567887654321,
            u64::MAX,
        ];
        for base in [0x2000203b, 0x2000403b, 0x0800403b] {
            for a in values {
                for b in values {
                    for rd in [0, 1, 2] {
                        let rs2 = if base == 0x0800403b { 0 } else { 2 };
                        let insn = base | (1 << 15) | (rs2 << 20) | (rd << 7);
                        let d = Decoded::new(insn);
                        assert!(!matches!(d.op, Op::Slow));
                        fast.x = [0; 32];
                        fast.x[1] = a;
                        fast.x[2] = b;
                        slow.x = fast.x;
                        fast.pc = RAM_BASE;
                        slow.pc = RAM_BASE;
                        assert_eq!(fast.execute_decoded(d, 4), slow.execute(insn, 4));
                        assert_eq!(fast.x, slow.x);
                        assert_eq!(fast.pc, slow.pc);
                    }
                }
            }
        }
        let invalid = 0x0800403b | (1 << 20);
        assert!(matches!(Decoded::new(invalid).op, Op::Slow));
        assert_eq!(
            fast.execute_decoded(Decoded::new(invalid), 4)
                .unwrap_err()
                .cause,
            2
        );
    }

    #[test]
    fn decoded_integer_execution_matches_original_executor() {
        let mut fast = Cpu::new(Memory::new(4096), RAM_BASE);
        let mut slow = Cpu::new(Memory::new(4096), RAM_BASE);
        let mut seed = 0x876543210abcdeffu64;
        let mut random = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let opcodes = [
            0x37, 0x17, 0x6f, 0x67, 0x63, 0x03, 0x23, 0x13, 0x1b, 0x33, 0x3b,
        ];
        for n in 0..20000 {
            let op = opcodes[n % opcodes.len()];
            let mut i = (random() as u32 & !127) | op;
            if op == 0x33 || op == 0x3b {
                i = (i & 0x1ffffff) | ([0, 1, 4, 16, 32][n % 5] << 25);
            }
            if op == 0x13 && matches!((i >> 12) & 7, 1 | 5) {
                i = (i & 0x3ffffff) | ([0, 16][n % 2] << 26);
            }
            if op == 0x1b && matches!((i >> 12) & 7, 1 | 5) {
                i = (i & 0x1ffffff) | ([0, 32][n % 2] << 25);
            }
            let mut regs = [0u64; 32];
            for x in &mut regs[1..] {
                *x = random();
            }
            if matches!(op, 3 | 0x23) && n % 2 == 0 {
                regs[((i >> 15) & 31) as usize] = RAM_BASE + 2048;
            }
            regs[0] = 0;
            fast.x = regs;
            slow.x = regs;
            fast.pc = RAM_BASE;
            slow.pc = RAM_BASE;
            fast.memory.ram.fill(0xa5);
            slow.memory.ram.fill(0xa5);
            let a = fast.execute_decoded(Decoded::new(i), 4);
            let b = slow.execute(i, 4);
            assert_eq!(a, b, "instruction {i:08x}");
            assert_eq!(fast.x, slow.x, "instruction {i:08x}");
            assert_eq!(fast.pc, slow.pc, "instruction {i:08x}");
            assert_eq!(fast.memory.ram, slow.memory.ram, "instruction {i:08x}");
        }
    }
}
