// Zba, Zbb, Zbs. Return None for encodings outside these extensions.
pub fn execute(i: u32, a: u64, b: u64) -> Option<u64> {
    let op = i & 127;
    let f = (i >> 12) & 7;
    let hi = i >> 25;
    let imm = i >> 20;
    Some(match (op, f, hi) {
        (0x33, 2 | 4 | 6, 0x10) => a.wrapping_shl(f / 2).wrapping_add(b),
        (0x3b, 2 | 4 | 6, 0x10) => (a as u32 as u64).wrapping_shl(f / 2).wrapping_add(b),
        (0x3b, 0, 4) => (a as u32 as u64).wrapping_add(b),
        (0x1b, 1, 4 | 5) => (a as u32 as u64) << ((i >> 20) & 63),
        (0x33, 4, 0x20) => a ^ !b,
        (0x33, 6, 0x20) => a | !b,
        (0x33, 7, 0x20) => a & !b,
        (0x33, 4, 5) => {
            if (a as i64) < (b as i64) {
                a
            } else {
                b
            }
        }
        (0x33, 5, 5) => a.min(b),
        (0x33, 6, 5) => {
            if (a as i64) > (b as i64) {
                a
            } else {
                b
            }
        }
        (0x33, 7, 5) => a.max(b),
        (0x33, 1, 0x30) => a.rotate_left((b & 63) as u32),
        (0x33, 5, 0x30) => a.rotate_right((b & 63) as u32),
        (0x3b, 1, 0x30) => (a as u32).rotate_left((b & 31) as u32) as i32 as u64,
        (0x3b, 5, 0x30) => (a as u32).rotate_right((b & 31) as u32) as i32 as u64,
        (0x13, 5, 0x30 | 0x31) => a.rotate_right(imm & 63),
        (0x1b, 5, 0x30) => (a as u32).rotate_right(imm & 31) as i32 as u64,
        (0x13, 1, 0x30) => match imm {
            0x600 => a.leading_zeros() as u64,
            0x601 => a.trailing_zeros() as u64,
            0x602 => a.count_ones() as u64,
            0x604 => a as i8 as u64,
            0x605 => a as i16 as u64,
            _ => return None,
        },
        (0x1b, 1, 0x30) => match imm {
            0x600 => (a as u32).leading_zeros() as u64,
            0x601 => (a as u32).trailing_zeros() as u64,
            0x602 => (a as u32).count_ones() as u64,
            _ => return None,
        },
        (0x3b, 4, 4) if imm & 31 == 0 => a as u16 as u64,
        (0x13, 5, 0x14) if imm == 0x287 => {
            let mut v = 0;
            for n in 0..8 {
                if (a >> (8 * n)) & 255 != 0 {
                    v |= 255 << (8 * n);
                }
            }
            v
        }
        (0x13, 5, 0x35) if imm == 0x6b8 => a.swap_bytes(),
        (0x33, 1, 0x14) => a | (1 << (b & 63)),
        (0x33, 1, 0x24) => a & !(1 << (b & 63)),
        (0x33, 1, 0x34) => a ^ (1 << (b & 63)),
        (0x33, 5, 0x24) => (a >> (b & 63)) & 1,
        (0x13, 1, 0x14 | 0x15) => a | (1 << (imm & 63)),
        (0x13, 1, 0x24 | 0x25) => a & !(1 << (imm & 63)),
        (0x13, 1, 0x34 | 0x35) => a ^ (1 << (imm & 63)),
        (0x13, 5, 0x24 | 0x25) => (a >> (imm & 63)) & 1,
        _ => return None,
    })
}
