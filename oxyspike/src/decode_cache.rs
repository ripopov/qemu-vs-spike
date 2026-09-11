use crate::arch::{access, exception};
use crate::csr;
use crate::{Cpu, Trap, compressed, fast::Decoded};
pub(crate) const ENTRIES: usize = 65_536;
// Indexing masks instead of dividing; keep this capacity a power of two.
const _: () = assert!(ENTRIES.is_power_of_two());
#[derive(Clone, Copy, Default)]
pub(crate) struct Entry {
    decoded: Decoded,
    pc: u64,
    epoch: u64,
    raw: u32,
    privilege: u8,
    len: u8,
}
pub(crate) fn new() -> Box<[Entry; ENTRIES]> {
    vec![Entry::default(); ENTRIES]
        .into_boxed_slice()
        .try_into()
        .ok()
        .unwrap()
}

impl Cpu {
    /// Invalidate decoded instructions without clearing the cache on each fence.
    pub fn flush_instruction(&mut self) {
        self.decode_satp = self.csrs[csr::SATP];
        self.decode_epoch = self.decode_epoch.wrapping_add(1);
        if self.decode_epoch == 0 {
            self.decoded.fill(Entry::default());
            self.decode_epoch = 1;
        }
    }

    #[inline(always)]
    pub(crate) fn execute_cached(&mut self) -> Result<(), Trap> {
        #[cfg(feature = "dispatch-profile")]
        {
            self.profile.lookups += 1;
        }
        // All entries share an address-space generation. Also observe direct
        // host writes to the public CSR array, which bypass csr_write.
        if self.decode_satp != self.csrs[csr::SATP] {
            self.flush_instruction();
        }
        let index = ((self.pc >> 1) as usize) & (ENTRIES - 1);
        let e = &self.decoded[index];
        if e.pc != self.pc || e.epoch != self.decode_epoch || e.privilege != self.privilege {
            #[cfg(feature = "dispatch-profile")]
            {
                self.profile.misses += 1;
            }
            self.fetch_decode_miss(index)?;
        }
        let e = self.decoded[index];
        self.execute_decoded(e.decoded, e.len as u64)
            .map_err(|mut trap| {
                if trap.cause == exception::ILLEGAL_INSTRUCTION {
                    trap.value = e.raw as u64;
                }
                trap
            })
    }

    #[cold]
    fn fetch_decode_miss(&mut self, index: usize) -> Result<(), Trap> {
        let pc = self.pc;
        let low = self.load_virtual(pc, 2, access::FETCH)? as u32;
        let (insn, raw, len) = if low & 3 == 3 {
            let raw =
                low | ((self.load_virtual(pc.wrapping_add(2), 2, access::FETCH)? as u32) << 16);
            // Spike fetches every parcel of long encodings before raising an
            // illegal-instruction trap. A later parcel may fault first.
            if low & 0x1f == 0x1f {
                let len = if low & 0x3f == 0x3f { 8 } else { 6 };
                for offset in (4..len).step_by(2) {
                    self.load_virtual(pc.wrapping_add(offset), 2, access::FETCH)?;
                }
                return Err(Trap {
                    cause: exception::ILLEGAL_INSTRUCTION,
                    value: raw as u64,
                });
            }
            (raw, raw, 4)
        } else {
            (
                compressed::expand(low as u16).ok_or(Trap {
                    cause: exception::ILLEGAL_INSTRUCTION,
                    value: low as u64,
                })?,
                low,
                2,
            )
        };
        let decoded = Decoded::new(insn);
        self.decoded[index] = Entry {
            decoded,
            pc,
            epoch: self.decode_epoch,
            raw,
            privilege: self.privilege,
            len,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, RAM_BASE};
    #[test]
    fn long_instruction_fetch_faults_precede_illegal_instruction() {
        for (low, available) in [(0x001f, 4), (0x003f, 4), (0x003f, 6), (0x007f, 6)] {
            let pc = RAM_BASE + 4096 - available;
            let mut c = Cpu::new(Memory::new(4096), pc);
            c.memory.store(pc, 2, low).unwrap();
            let trap = c.step().unwrap_err();
            assert_eq!((trap.cause, trap.value), (1, RAM_BASE + 4096));
            assert_eq!(c.pc, pc);
            assert_eq!(c.retired, 0);
        }
    }

    #[test]
    fn cached_compressed_fallback_preserves_encoding_and_trap_value() {
        let mut c = Cpu::new(Memory::new(4096), RAM_BASE);
        // C.FLD f8,0(x8) expands to an instruction handled by the slow executor.
        c.memory.store(RAM_BASE, 2, 0x2000).unwrap();
        c.x[8] = RAM_BASE + 128;
        let bits = 0x8000000000000000;
        c.memory.store(c.x[8], 8, bits).unwrap();
        for _ in 0..2 {
            let trap = c.step().unwrap_err();
            assert_eq!((trap.cause, trap.value), (2, 0x2000));
            assert_eq!(c.pc, RAM_BASE);
        }
        c.csr_write(0x300, 1 << 13);
        c.step().unwrap();
        assert_eq!(c.f[8], bits);
        assert_eq!(c.pc, RAM_BASE + 2);
    }

    #[test]
    fn epoch_rollover_cannot_resurrect_an_old_instruction() {
        let mut c = Cpu::new(Memory::new(4096), RAM_BASE);
        c.memory.store(RAM_BASE, 4, 0x00100093).unwrap();
        c.step().unwrap();
        assert_eq!(c.x[1], 1);
        c.memory.store(RAM_BASE, 4, 0x00200093).unwrap();
        c.decode_epoch = u64::MAX;
        c.flush_instruction();
        c.pc = RAM_BASE;
        c.step().unwrap();
        assert_eq!(c.x[1], 2);
    }
}
