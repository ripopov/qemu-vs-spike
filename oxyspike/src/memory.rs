use crate::Trap;
use crate::arch::exception;
pub const RAM_BASE: u64 = 0x8000_0000;
const CLINT_BASE: u64 = 0x0200_0000;
const CLINT_END: u64 = CLINT_BASE + 0xc000;
const MSIP_LAST: u64 = CLINT_BASE + 3;
const MTIMECMP: u64 = CLINT_BASE + 0x4000;
const MTIMECMP_LAST: u64 = MTIMECMP + 7;
const MTIME: u64 = CLINT_BASE + 0xbff8;
const MTIME_LAST: u64 = MTIME + 7;

/// RAM and the single-hart CLINT/HTIF state used by the simulated platform.
pub struct Memory {
    pub ram: Vec<u8>,
    pub mtime: u64,
    pub mtimecmp: u64,
    pub msip: u32,
    pub tohost: u64,
    pub htif_pending: bool,
    pub interrupt_dirty: bool,
}

impl Memory {
    pub fn new(size: usize) -> Self {
        Self {
            ram: vec![0; size],
            mtime: 0,
            mtimecmp: 0,
            msip: 0,
            tohost: u64::MAX,
            htif_pending: false,
            interrupt_dirty: true,
        }
    }

    pub fn advance_time(&mut self, ticks: u64) {
        self.mtime = self.mtime.wrapping_add(ticks);
        self.interrupt_dirty = true;
    }

    #[inline(always)]
    fn offset(&self, address: u64, size: usize, cause: u64) -> Result<usize, Trap> {
        let offset = address.wrapping_sub(RAM_BASE);
        if offset
            .checked_add(size as u64)
            .is_none_or(|end| end > self.ram.len() as u64)
        {
            Err(Trap {
                cause,
                value: address,
            })
        } else {
            Ok(offset as usize)
        }
    }

    #[inline]
    pub(crate) fn reservable(&self, address: u64, size: usize) -> bool {
        self.offset(address, size, exception::LOAD_ACCESS).is_ok()
    }

    #[inline(always)]
    pub fn load(&self, address: u64, size: usize) -> Result<u64, Trap> {
        if (CLINT_BASE..CLINT_END).contains(&address) {
            return self.load_clint(address, size);
        }
        let o = self.offset(address, size, exception::LOAD_ACCESS)?;
        Ok(match size {
            1 => self.ram[o] as u64,
            2 => u16::from_le_bytes(self.ram[o..o + 2].try_into().unwrap()) as u64,
            4 => u32::from_le_bytes(self.ram[o..o + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(self.ram[o..o + 8].try_into().unwrap()),
            _ => unreachable!(),
        })
    }
    // Cross-page accesses can leave non-power-of-two fragments in RAM.
    #[cold]
    pub(crate) fn load_fragment(&self, address: u64, size: usize) -> Result<u64, Trap> {
        if (CLINT_BASE..CLINT_END).contains(&address) {
            return self.load_clint(address, size);
        }
        let o = self.offset(address, size, exception::LOAD_ACCESS)?;
        let mut bytes = [0; 8];
        bytes[..size].copy_from_slice(&self.ram[o..o + size]);
        Ok(u64::from_le_bytes(bytes))
    }

    #[inline(always)]
    pub fn store(&mut self, address: u64, size: usize, value: u64) -> Result<(), Trap> {
        if (CLINT_BASE..CLINT_END).contains(&address) {
            return self.store_clint(address, size, value);
        }
        let o = self.offset(address, size, exception::STORE_ACCESS)?;
        self.ram[o..o + size].copy_from_slice(&value.to_le_bytes()[..size]);
        self.htif_pending |= address.wrapping_sub(self.tohost) < 8
            || self.tohost.wrapping_sub(address) < size as u64;
        Ok(())
    }

    #[cold]
    fn load_clint(&self, address: u64, size: usize) -> Result<u64, Trap> {
        if !matches!(size, 1 | 2 | 4 | 8) {
            return Err(Trap {
                cause: exception::LOAD_ACCESS,
                value: address,
            });
        }
        if address < MTIMECMP && size == 8 {
            return Ok(self.load_clint(address, 4)? | (self.load_clint(address + 4, 4)? << 32));
        }
        let value = match address {
            // A double-word MSIP read is two word reads; hart 1 is absent.
            CLINT_BASE..=MSIP_LAST => {
                (self.msip & 1).rotate_right(((address & 3) * 8) as u32) as u64
            }
            MTIMECMP..=MTIMECMP_LAST => self.mtimecmp.rotate_right(((address & 7) * 8) as u32),
            MTIME..=MTIME_LAST => self.mtime.rotate_right(((address & 7) * 8) as u32),
            _ => 0, // Registers for absent harts read as zero.
        };
        Ok(value & (u64::MAX >> (64 - size * 8)))
    }

    #[cold]
    fn store_clint(&mut self, address: u64, size: usize, value: u64) -> Result<(), Trap> {
        if !matches!(size, 1 | 2 | 4 | 8) {
            return Err(Trap {
                cause: exception::STORE_ACCESS,
                value: address,
            });
        }
        if address < MTIMECMP && size == 8 {
            self.store_clint(address, 4, value)?;
            return self.store_clint(address + 4, 4, value >> 32);
        }
        match address {
            CLINT_BASE => self.msip = value as u32 & 1,
            MTIMECMP..=MTIMECMP_LAST | MTIME..=MTIME_LAST => {
                let reg = if address < MTIME {
                    &mut self.mtimecmp
                } else {
                    &mut self.mtime
                };
                for n in 0..size {
                    let shift = ((address + n as u64) & 7) * 8;
                    *reg = (*reg & !(255 << shift)) | (((value >> (n * 8)) & 255) << shift);
                }
            }
            _ => return Ok(()), // Upper MSIP bytes and absent harts ignore writes.
        }
        self.interrupt_dirty = true;
        Ok(())
    }
    pub(crate) fn zero_range(&mut self, address: u64, size: usize) -> Result<(), String> {
        let o = self
            .offset(address, size, exception::STORE_ACCESS)
            .map_err(|e| format!("{e:?}"))?;
        self.ram[o..o + size].fill(0);
        Ok(())
    }

    pub fn copy_in(&mut self, address: u64, bytes: &[u8]) -> Result<(), String> {
        let o = self
            .offset(address, bytes.len(), exception::STORE_ACCESS)
            .map_err(|e| format!("{e:?}"))?;
        self.ram[o..o + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_bounds_match_wide_interval_arithmetic() {
        for capacity in [0, 1, 7, 8, 64] {
            let memory = Memory::new(capacity);
            for size in [0, 1, 2, 4, 8, 65, usize::MAX] {
                for address in [
                    0,
                    RAM_BASE - 8,
                    RAM_BASE - 1,
                    RAM_BASE,
                    RAM_BASE + capacity as u64,
                    RAM_BASE + capacity as u64 + 1,
                    u64::MAX,
                ] {
                    let fits = address >= RAM_BASE
                        && address as u128 + size as u128 <= RAM_BASE as u128 + capacity as u128;
                    let want = if fits {
                        Ok((address - RAM_BASE) as usize)
                    } else {
                        Err(Trap {
                            cause: exception::LOAD_ACCESS,
                            value: address,
                        })
                    };
                    assert_eq!(
                        memory.offset(address, size, exception::LOAD_ACCESS),
                        want,
                        "capacity={capacity}, address={address:#x}, size={size}"
                    );
                }
            }
        }
    }
}
