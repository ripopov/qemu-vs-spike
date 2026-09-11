use crate::arch::exception;
use crate::{
    Cpu, Trap, csr,
    softfloat::{self as sf, DOUBLE, Format, HALF, SINGLE},
    status,
};
impl Cpu {
    fn fp_read(&self, reg: usize, f: Format) -> u64 {
        let v = self.f[reg];
        if f.frac == HALF.frac {
            if v >> 16 == 0xffffffffffff {
                v as u16 as u64
            } else {
                f.nan()
            }
        } else if f.frac == SINGLE.frac {
            if v >> 32 == u32::MAX as u64 {
                v as u32 as u64
            } else {
                f.nan()
            }
        } else {
            v
        }
    }
    pub(crate) fn floating(&mut self, i: u32) -> Result<Option<u64>, Trap> {
        let illegal = Trap {
            cause: exception::ILLEGAL_INSTRUCTION,
            value: i as u64,
        };
        if self.csrs[csr::MSTATUS] & status::FS == 0 {
            return Err(illegal);
        }
        let rd = ((i >> 7) & 31) as usize;
        let rs1 = ((i >> 15) & 31) as usize;
        let rs2 = ((i >> 20) & 31) as usize;
        let funct = (i >> 12) & 7;
        let hi = i >> 25;
        let op = i & 127;
        let f = match (i >> 25) & 3 {
            0 => SINGLE,
            1 => DOUBLE,
            2 => HALF,
            _ => return Err(illegal),
        };
        if f.frac == HALF.frac && (op != 0x53 || !matches!(hi, 0x22 | 0x72 | 0x7a)) {
            return Err(illegal);
        }
        let a = self.fp_read(rs1, f);
        let b = self.fp_read(rs2, f);
        let rounding = if funct == 7 {
            ((self.csrs[csr::FCSR] >> 5) & 7) as u32
        } else {
            funct
        };
        let needs_round =
            op != 0x53 || matches!(hi & !3, 0 | 4 | 8 | 12 | 0x20 | 0x2c | 0x60 | 0x68);
        if needs_round && rounding > sf::rounding::NEAREST_MAX_MAGNITUDE {
            return Err(illegal);
        }
        let mut integer = false;
        let (value, flags) = if op != 0x53 {
            let c = self.fp_read((i >> 27) as usize, f);
            let a = if matches!(op, 0x4b | 0x4f) {
                a ^ f.sign()
            } else {
                a
            };
            let c = if matches!(op, 0x47 | 0x4f) {
                c ^ f.sign()
            } else {
                c
            };
            sf::fma(f, a, b, c, rounding)
        } else {
            match hi & !3 {
                0 => sf::add(f, a, b, rounding),
                4 => sf::add(f, a, b ^ f.sign(), rounding),
                8 => sf::mul(f, a, b, rounding),
                12 => sf::div(f, a, b, rounding),
                0x2c if rs2 == 0 => sf::sqrt(f, a, rounding),
                0x10 if funct <= 2 => (
                    (a & !f.sign())
                        | match funct {
                            0 => b & f.sign(),
                            1 => (!b) & f.sign(),
                            _ => (a ^ b) & f.sign(),
                        },
                    0,
                ),
                0x14 if funct <= 1 => sf::minmax(f, a, b, funct == 1),
                0x20 if rs2 <= 2 && rs2 != ((i >> 25) & 3) as usize => {
                    let from = [SINGLE, DOUBLE, HALF][rs2];
                    sf::convert(from, f, self.fp_read(rs1, from), rounding)
                }
                0x50 if funct <= 2 => {
                    integer = true;
                    sf::compare(f, a, b, funct)
                }
                0x60 if rs2 <= 3 => {
                    integer = true;
                    sf::to_int(f, a, rs2 & 1 == 0, rs2 < 2, rounding)
                }
                0x68 if rs2 <= 3 => sf::from_int(f, self.x[rs1], rs2 & 1 == 0, rs2 < 2, rounding),
                0x70 if rs2 == 0 && funct <= 1 && (f.frac != HALF.frac || funct == 0) => {
                    integer = true;
                    (
                        if funct == 1 {
                            f.classify(a)
                        } else if f.frac == HALF.frac {
                            self.f[rs1] as i16 as u64
                        } else if f.frac == SINGLE.frac {
                            self.f[rs1] as i32 as u64
                        } else {
                            self.f[rs1]
                        },
                        0,
                    )
                }
                0x78 if rs2 == 0 && funct == 0 => (self.x[rs1], 0),
                _ => return Err(illegal),
            }
        };
        if flags != 0 {
            self.csrs[csr::FCSR] |= flags as u64;
            self.csrs[csr::MSTATUS] |= status::FS;
        }
        if integer {
            Ok(Some(value))
        } else {
            self.f[rd] = if f.frac == HALF.frac {
                value as u16 as u64 | 0xffffffffffff0000
            } else if f.frac == SINGLE.frac {
                value as u32 as u64 | 0xffffffff00000000
            } else {
                value
            };
            self.csrs[csr::MSTATUS] |= status::FS;
            Ok(None)
        }
    }
}
