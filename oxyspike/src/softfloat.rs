//! Exact binary arithmetic and a single final IEEE-754 rounding step.
//! Integer significands avoid dependence on the host floating-point environment.
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};
// RISC-V fflags encodings.
pub const NX: u8 = 1 << 0;
pub const UF: u8 = 1 << 1;
pub const OF: u8 = 1 << 2;
pub const DZ: u8 = 1 << 3;
pub const NV: u8 = 1 << 4;

/// RISC-V rounding-mode encodings; dynamic rounding is resolved by the CPU.
pub mod rounding {
    pub const NEAREST_EVEN: u32 = 0;
    pub const TOWARD_ZERO: u32 = 1;
    pub const DOWN: u32 = 2;
    pub const UP: u32 = 3;
    pub const NEAREST_MAX_MAGNITUDE: u32 = 4;
}
use rounding::{DOWN, NEAREST_EVEN, NEAREST_MAX_MAGNITUDE, TOWARD_ZERO, UP};

/// Fraction and exponent widths of an IEEE binary interchange format.
#[derive(Clone, Copy)]
pub struct Format {
    pub frac: u32,
    pub exp: u32,
}

pub const HALF: Format = Format { frac: 10, exp: 5 };
pub const SINGLE: Format = Format { frac: 23, exp: 8 };
pub const DOUBLE: Format = Format { frac: 52, exp: 11 };
impl Format {
    pub fn sign(self) -> u64 {
        1 << (self.frac + self.exp)
    }

    pub fn inf(self) -> u64 {
        ((1 << self.exp) - 1) << self.frac
    }

    pub fn nan(self) -> u64 {
        self.inf() | (1 << (self.frac - 1))
    }

    pub fn is_nan(self, x: u64) -> bool {
        x & self.inf() == self.inf() && x & ((1 << self.frac) - 1) != 0
    }

    pub fn is_snan(self, x: u64) -> bool {
        self.is_nan(x) && x & (1 << (self.frac - 1)) == 0
    }

    fn is_inf(self, x: u64) -> bool {
        x & !self.sign() == self.inf()
    }

    fn is_zero(self, x: u64) -> bool {
        x & !self.sign() == 0
    }

    fn bias(self) -> i32 {
        (1 << (self.exp - 1)) - 1
    }

    fn finite(self, x: u64) -> (bool, BigUint, i32) {
        let e = ((x >> self.frac) & ((1 << self.exp) - 1)) as i32;
        let m = (x & ((1 << self.frac) - 1)) | if e != 0 { 1 << self.frac } else { 0 };
        (
            x & self.sign() != 0,
            BigUint::from(m),
            e.max(1) - self.bias() - self.frac as i32,
        )
    }

    pub fn classify(self, x: u64) -> u64 {
        let sign = x & self.sign() != 0;
        1 << if self.is_nan(x) {
            if self.is_snan(x) { 8 } else { 9 }
        } else if self.is_inf(x) {
            if sign { 0 } else { 7 }
        } else if self.is_zero(x) {
            if sign { 3 } else { 4 }
        } else if x & self.inf() == 0 {
            if sign { 2 } else { 5 }
        } else if sign {
            1
        } else {
            6
        }
    }
}
// Return quotient, remainder and denominator for (n / d) * 2^shift.
fn scaled_div(n: BigUint, d: BigUint, shift: i32) -> (BigUint, BigUint, BigUint) {
    if d.is_one() {
        if shift >= 0 {
            return (n << shift as usize, BigUint::zero(), d);
        }
        let shift = (-shift) as usize;
        let q = &n >> shift;
        let r = n - (&q << shift);
        return (q, r, d << shift);
    }
    let (n, d) = if shift >= 0 {
        (n << (shift as usize), d)
    } else {
        (n, d << ((-shift) as usize))
    };
    let (q, r) = n.div_rem(&d);
    (q, r, d)
}

fn rounded(mut q: BigUint, r: &BigUint, d: &BigUint, sign: bool, rm: u32) -> BigUint {
    if r.is_zero() {
        return q;
    }
    let inc = match rm {
        NEAREST_EVEN => {
            let twice = r << 1usize;
            twice > *d || twice == *d && q.bit(0)
        }
        TOWARD_ZERO => false,
        DOWN => sign,
        UP => !sign,
        NEAREST_MAX_MAGNITUDE => (r << 1usize) >= *d,
        _ => unreachable!(),
    };
    if inc {
        q += 1u32;
    }
    q
}
// Round the exact value (-1)^sign * (n / d) * 2^e once.
fn round(f: Format, sign: bool, n: BigUint, d: BigUint, e: i32, rm: u32) -> (u64, u8) {
    let signbit = if sign { f.sign() } else { 0 };
    if n.is_zero() {
        return (signbit, 0);
    }
    let mut log = n.bits() as i32 - d.bits() as i32;
    // For n / 1, bit length already gives floor(log2(n)) exactly.
    if !d.is_one() {
        let below = if log >= 0 {
            n < (&d << (log as usize))
        } else {
            (&n << ((-log) as usize)) < d
        };
        if below {
            log -= 1;
        }
    }
    log += e;
    // Tininess after rounding uses precision with an unbounded exponent range,
    // before the reduced subnormal precision is applied (as in Spike SoftFloat).
    let min_exp = 1 - f.bias();
    let tiny = log < min_exp
        && (log < min_exp - 1 || {
            let (q, r, d) = scaled_div(n.clone(), d.clone(), e - (log - f.frac as i32));
            rounded(q, &r, &d, sign, rm).bits() <= f.frac as u64 + 1
        });
    let qexp = (log - f.frac as i32).max(1 - f.bias() - f.frac as i32);
    let (q, r, d) = scaled_div(n, d, e - qexp);
    let inexact = !r.is_zero();
    let mut q = rounded(q, &r, &d, sign, rm);
    let mut out_exp = qexp;
    if q.bits() > f.frac as u64 + 1 {
        q >>= 1usize;
        out_exp += 1;
    }
    let normal = q.bits() > f.frac as u64;
    let encoded_exp = if normal {
        out_exp + f.frac as i32 + f.bias()
    } else {
        0
    };
    if encoded_exp >= (1 << f.exp) - 1 {
        let inf = matches!(rm, NEAREST_EVEN | NEAREST_MAX_MAGNITUDE)
            || rm == DOWN && sign
            || rm == UP && !sign;
        return (signbit | if inf { f.inf() } else { f.inf() - 1 }, OF | NX);
    }
    let bits =
        signbit | ((encoded_exp as u64) << f.frac) | (q.to_u64().unwrap() & ((1 << f.frac) - 1));
    (
        bits,
        if inexact {
            NX | if tiny { UF } else { 0 }
        } else {
            0
        },
    )
}
// Align exact significands before addition, preserving the sign of exact zero.
fn sum(
    (a_sign, a_mantissa, a_exp): (bool, BigUint, i32),
    (b_sign, b_mantissa, b_exp): (bool, BigUint, i32),
    rm: u32,
) -> (bool, BigUint, i32) {
    let exponent = a_exp.min(b_exp);
    let a = a_mantissa << ((a_exp - exponent) as usize);
    let b = b_mantissa << ((b_exp - exponent) as usize);
    if a_sign == b_sign {
        (a_sign, a + b, exponent)
    } else if a > b {
        (a_sign, a - b, exponent)
    } else if b > a {
        (b_sign, b - a, exponent)
    } else {
        (rm == DOWN, BigUint::zero(), exponent)
    }
}

pub fn add(f: Format, a: u64, b: u64, rm: u32) -> (u64, u8) {
    if f.is_nan(a) || f.is_nan(b) {
        return (f.nan(), if f.is_snan(a) || f.is_snan(b) { NV } else { 0 });
    }
    if f.is_inf(a) || f.is_inf(b) {
        return if f.is_inf(a) && f.is_inf(b) && (a ^ b) & f.sign() != 0 {
            (f.nan(), NV)
        } else {
            (if f.is_inf(a) { a } else { b }, 0)
        };
    }
    let (s, n, e) = sum(f.finite(a), f.finite(b), rm);
    round(f, s, n, BigUint::one(), e, rm)
}

pub fn mul(f: Format, a: u64, b: u64, rm: u32) -> (u64, u8) {
    if f.is_nan(a) || f.is_nan(b) {
        return (f.nan(), if f.is_snan(a) || f.is_snan(b) { NV } else { 0 });
    }
    let s = (a ^ b) & f.sign();
    if f.is_inf(a) || f.is_inf(b) {
        return if f.is_zero(a) || f.is_zero(b) {
            (f.nan(), NV)
        } else {
            (s | f.inf(), 0)
        };
    }
    let (_, an, ae) = f.finite(a);
    let (_, bn, be) = f.finite(b);
    round(f, s != 0, an * bn, BigUint::one(), ae + be, rm)
}

pub fn div(f: Format, a: u64, b: u64, rm: u32) -> (u64, u8) {
    if f.is_nan(a) || f.is_nan(b) {
        return (f.nan(), if f.is_snan(a) || f.is_snan(b) { NV } else { 0 });
    }
    let s = (a ^ b) & f.sign();
    if f.is_inf(a) && f.is_inf(b) || f.is_zero(a) && f.is_zero(b) {
        return (f.nan(), NV);
    }
    if f.is_inf(a) {
        return (s | f.inf(), 0);
    }
    if f.is_inf(b) {
        return (s, 0);
    }
    if f.is_zero(b) {
        return (s | f.inf(), DZ);
    }
    let (_, an, ae) = f.finite(a);
    let (_, bn, be) = f.finite(b);
    round(f, s != 0, an, bn, ae - be, rm)
}

pub fn fma(f: Format, a: u64, b: u64, c: u64, rm: u32) -> (u64, u8) {
    let invalid = f.is_snan(a)
        || f.is_snan(b)
        || f.is_snan(c)
        || f.is_inf(a) && f.is_zero(b)
        || f.is_inf(b) && f.is_zero(a);
    if invalid || f.is_nan(a) || f.is_nan(b) || f.is_nan(c) {
        return (f.nan(), if invalid { NV } else { 0 });
    }
    let sign = (a ^ b) & f.sign();
    if f.is_inf(a) || f.is_inf(b) {
        return if f.is_inf(c) && (c ^ sign) & f.sign() != 0 {
            (f.nan(), NV)
        } else {
            (sign | f.inf(), 0)
        };
    }
    if f.is_inf(c) {
        return (c, 0);
    }
    let (asign, an, ae) = f.finite(a);
    let (bsign, bn, be) = f.finite(b);
    let (s, n, e) = sum((asign ^ bsign, an * bn, ae + be), f.finite(c), rm);
    round(f, s, n, BigUint::one(), e, rm)
}

pub fn sqrt(f: Format, a: u64, rm: u32) -> (u64, u8) {
    if f.is_nan(a) {
        return (f.nan(), if f.is_snan(a) { NV } else { 0 });
    }
    if f.is_zero(a) {
        return (a, 0);
    }
    if a & f.sign() != 0 {
        return (f.nan(), NV);
    }
    if f.is_inf(a) {
        return (a, 0);
    }
    let (_, n, e) = f.finite(a);
    let log = (n.bits() as i32 - 1 + e).div_euclid(2);
    let qexp = (log - f.frac as i32).max(1 - f.bias() - f.frac as i32);
    let shift = e - 2 * qexp;
    let (n, d) = if shift >= 0 {
        (n << (shift as usize), BigUint::one())
    } else {
        (n, BigUint::one() << ((-shift) as usize))
    };
    let mut q = (&n / &d).sqrt();
    let exact = &q * &q * &d == n;
    if !exact {
        let up = match rm {
            NEAREST_EVEN | NEAREST_MAX_MAGNITUDE => {
                let middle = (&q << 1usize) + 1u32;
                (&n << 2usize) > (&middle * &middle * &d)
            }
            TOWARD_ZERO | DOWN => false,
            UP => true,
            _ => unreachable!(),
        };
        if up {
            q += 1u32;
        }
    }
    let (out, flags) = round(f, false, q, BigUint::one(), qexp, NEAREST_EVEN);
    (out, flags | u8::from(!exact))
}

pub fn convert(from: Format, to: Format, a: u64, rm: u32) -> (u64, u8) {
    let sign = if a & from.sign() != 0 { to.sign() } else { 0 };
    if from.is_nan(a) {
        return (to.nan(), if from.is_snan(a) { NV } else { 0 });
    }
    if from.is_inf(a) {
        return (sign | to.inf(), 0);
    }
    let (s, n, e) = from.finite(a);
    round(to, s, n, BigUint::one(), e, rm)
}

pub fn from_int(f: Format, a: u64, signed: bool, word: bool, rm: u32) -> (u64, u8) {
    let a = if word {
        if signed {
            a as i32 as u64
        } else {
            a as u32 as u64
        }
    } else {
        a
    };
    let sign = signed && (a as i64) < 0;
    let n = if sign { a.wrapping_neg() } else { a };
    round(f, sign, BigUint::from(n), BigUint::one(), 0, rm)
}

pub fn to_int(f: Format, a: u64, signed: bool, word: bool, rm: u32) -> (u64, u8) {
    let width = if word { 32 } else { 64 };
    let min = if signed { 1u64 << (width - 1) } else { 0 };
    let max = if signed {
        min - 1
    } else if word {
        u32::MAX as u64
    } else {
        u64::MAX
    };
    let extend = |x: u64| if word { x as i32 as u64 } else { x };
    if f.is_nan(a) {
        return (extend(max), NV);
    }
    let sign = a & f.sign() != 0;
    if f.is_inf(a) {
        return (extend(if sign { min } else { max }), NV);
    }
    let (_, n, e) = f.finite(a);
    let (q, r, d) = scaled_div(n, BigUint::one(), e);
    let q = rounded(q, &r, &d, sign, rm);
    if q > BigUint::from(if sign { min } else { max }) {
        return (extend(if sign { min } else { max }), NV);
    }
    let v = q.to_u64().unwrap();
    (
        extend(if sign { v.wrapping_neg() } else { v }),
        u8::from(!r.is_zero()),
    )
}

pub fn compare(f: Format, a: u64, b: u64, op: u32) -> (u64, u8) {
    if f.is_nan(a) || f.is_nan(b) {
        return (
            0,
            if op != 2 || f.is_snan(a) || f.is_snan(b) {
                NV
            } else {
                0
            },
        );
    }
    let equal = a == b || f.is_zero(a) && f.is_zero(b);
    let less = if equal {
        false
    } else if (a ^ b) & f.sign() != 0 {
        a & f.sign() != 0
    } else if a & f.sign() != 0 {
        a > b
    } else {
        a < b
    };
    (
        match op {
            0 => equal || less,
            1 => less,
            _ => equal,
        } as u64,
        0,
    )
}

pub fn minmax(f: Format, a: u64, b: u64, max: bool) -> (u64, u8) {
    let flags = if f.is_snan(a) || f.is_snan(b) { NV } else { 0 };
    if f.is_nan(a) && f.is_nan(b) {
        return (f.nan(), flags);
    }
    if f.is_nan(a) {
        return (b, flags);
    }
    if f.is_nan(b) {
        return (a, flags);
    }
    let less = compare(f, a, b, 1).0 != 0 || f.is_zero(a) && f.is_zero(b) && a & f.sign() != 0;
    (if less ^ max { a } else { b }, flags)
}
