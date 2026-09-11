//! Optional operation counters; excluded from normal release builds.
use crate::Cpu;
use std::fmt::Write;

pub(crate) struct DispatchProfile {
    pub lookups: u64,
    pub misses: u64,
    pub operations: [u64; OP_NAMES.len()],
    pub slow_opcodes: [u64; 128],
    pub slow_functions: Box<[u64; 4096]>,
}

impl Default for DispatchProfile {
    fn default() -> Self {
        Self {
            lookups: 0,
            misses: 0,
            operations: [0; OP_NAMES.len()],
            slow_opcodes: [0; 128],
            slow_functions: vec![0; 4096].into_boxed_slice().try_into().unwrap(),
        }
    }
}

const OP_NAMES: &[&str] = &[
    "Slow", "Lui", "Auipc", "Jal", "Jalr", "Beq", "Bne", "Blt", "Bge", "Bltu", "Bgeu", "Lb", "Lh",
    "Lw", "Ld", "Lbu", "Lhu", "Lwu", "Sb", "Sh", "Sw", "Sd", "Addi", "Slti", "Sltiu", "Xori",
    "Ori", "Andi", "Slli", "Srli", "Srai", "Addiw", "Slliw", "Srliw", "Sraiw", "Add", "Sub", "Sll",
    "Slt", "Sltu", "Xor", "Srl", "Sra", "Or", "And", "Mul", "Addw", "Subw", "Sllw", "Srlw", "Sraw",
    "Mulw", "Sh1addUw", "Sh2addUw", "ZextH",
];
// Keep counter indices and display names synchronized with the enum.
const _: () = {
    assert!(crate::fast::Op::Slow as usize == 0);
    assert!(crate::fast::Op::Lui as usize == 1);
    assert!(crate::fast::Op::Auipc as usize == 2);
    assert!(crate::fast::Op::Jal as usize == 3);
    assert!(crate::fast::Op::Jalr as usize == 4);
    assert!(crate::fast::Op::Beq as usize == 5);
    assert!(crate::fast::Op::Bne as usize == 6);
    assert!(crate::fast::Op::Blt as usize == 7);
    assert!(crate::fast::Op::Bge as usize == 8);
    assert!(crate::fast::Op::Bltu as usize == 9);
    assert!(crate::fast::Op::Bgeu as usize == 10);
    assert!(crate::fast::Op::Lb as usize == 11);
    assert!(crate::fast::Op::Lh as usize == 12);
    assert!(crate::fast::Op::Lw as usize == 13);
    assert!(crate::fast::Op::Ld as usize == 14);
    assert!(crate::fast::Op::Lbu as usize == 15);
    assert!(crate::fast::Op::Lhu as usize == 16);
    assert!(crate::fast::Op::Lwu as usize == 17);
    assert!(crate::fast::Op::Sb as usize == 18);
    assert!(crate::fast::Op::Sh as usize == 19);
    assert!(crate::fast::Op::Sw as usize == 20);
    assert!(crate::fast::Op::Sd as usize == 21);
    assert!(crate::fast::Op::Addi as usize == 22);
    assert!(crate::fast::Op::Slti as usize == 23);
    assert!(crate::fast::Op::Sltiu as usize == 24);
    assert!(crate::fast::Op::Xori as usize == 25);
    assert!(crate::fast::Op::Ori as usize == 26);
    assert!(crate::fast::Op::Andi as usize == 27);
    assert!(crate::fast::Op::Slli as usize == 28);
    assert!(crate::fast::Op::Srli as usize == 29);
    assert!(crate::fast::Op::Srai as usize == 30);
    assert!(crate::fast::Op::Addiw as usize == 31);
    assert!(crate::fast::Op::Slliw as usize == 32);
    assert!(crate::fast::Op::Srliw as usize == 33);
    assert!(crate::fast::Op::Sraiw as usize == 34);
    assert!(crate::fast::Op::Add as usize == 35);
    assert!(crate::fast::Op::Sub as usize == 36);
    assert!(crate::fast::Op::Sll as usize == 37);
    assert!(crate::fast::Op::Slt as usize == 38);
    assert!(crate::fast::Op::Sltu as usize == 39);
    assert!(crate::fast::Op::Xor as usize == 40);
    assert!(crate::fast::Op::Srl as usize == 41);
    assert!(crate::fast::Op::Sra as usize == 42);
    assert!(crate::fast::Op::Or as usize == 43);
    assert!(crate::fast::Op::And as usize == 44);
    assert!(crate::fast::Op::Mul as usize == 45);
    assert!(crate::fast::Op::Addw as usize == 46);
    assert!(crate::fast::Op::Subw as usize == 47);
    assert!(crate::fast::Op::Sllw as usize == 48);
    assert!(crate::fast::Op::Srlw as usize == 49);
    assert!(crate::fast::Op::Sraw as usize == 50);
    assert!(crate::fast::Op::Mulw as usize == 51);
    assert!(crate::fast::Op::Sh1addUw as usize == 52);
    assert!(crate::fast::Op::Sh2addUw as usize == 53);
    assert!(crate::fast::Op::ZextH as usize == 54);
};

impl Cpu {
    /// Diagnostic counts of dispatch attempts (including execution traps), not
    /// retired instructions. Misses include fetch/decode faults. Slow opcodes
    /// are the low seven bits of the expanded instruction encoding.
    pub fn dispatch_profile_json(&self) -> String {
        let p = &self.profile;
        let mut out = format!(
            "{{\"lookups\":{},\"misses\":{},\"operations\":{{",
            p.lookups, p.misses
        );
        for (i, (name, count)) in OP_NAMES.iter().zip(p.operations).enumerate() {
            if i != 0 {
                out.push(',');
            }
            write!(out, "\"{name}\":{count}").unwrap();
        }
        out.push_str("},\"slow_opcodes\":[");
        for (i, count) in p.slow_opcodes.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            write!(out, "{count}").unwrap();
        }
        out.push_str("],\"slow_functions\":[");
        for (i, count) in p.slow_functions.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            write!(out, "{count}").unwrap();
        }
        out.push_str("]}");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, RAM_BASE};

    #[test]
    fn counts_cache_hits_execution_traps_and_fetch_failures_separately() {
        let mut c = Cpu::new(Memory::new(4096), RAM_BASE);
        c.memory.store(RAM_BASE, 4, 0x00100093).unwrap();
        c.memory.store(RAM_BASE + 4, 4, 0x00000073).unwrap();
        c.step().unwrap();
        c.pc = RAM_BASE;
        c.step().unwrap();
        assert_eq!(c.step().unwrap_err().cause, 11); // ECALL executes, then traps.
        c.pc = RAM_BASE + 8;
        assert_eq!(c.step().unwrap_err().cause, 2); // Invalid compressed encoding.
        assert_eq!((c.profile.lookups, c.profile.misses), (4, 3));
        assert_eq!(c.profile.operations[crate::fast::Op::Addi as usize], 2);
        assert_eq!(c.profile.operations[crate::fast::Op::Slow as usize], 1);
        assert_eq!(c.profile.operations.iter().sum::<u64>(), 3);
        assert_eq!(c.profile.slow_opcodes[0x73], 1);
    }
}
