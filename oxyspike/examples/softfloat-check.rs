use oxyspike::softfloat::{self as sf, DOUBLE, HALF, SINGLE};
use std::io::{self, BufRead, Write};
fn main() {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let p: Vec<_> = line.split_whitespace().collect();
        let fmt = p[0].parse::<u32>().unwrap();
        let op = p[1].parse::<u32>().unwrap();
        let rm = p[2].parse::<u32>().unwrap();
        let a = u64::from_str_radix(p[3], 16).unwrap();
        let b = u64::from_str_radix(p[4], 16).unwrap();
        let c = u64::from_str_radix(p[5], 16).unwrap();
        let f = match fmt {
            16 => HALF,
            32 => SINGLE,
            _ => DOUBLE,
        };
        let (v, flags) = match op {
            0 => sf::add(f, a, b, rm),
            1 => sf::mul(f, a, b, rm),
            2 => sf::div(f, a, b, rm),
            3 => sf::sqrt(f, a, rm),
            4 => sf::fma(f, a, b, c, rm),
            5 => sf::convert(f, if fmt == 32 { DOUBLE } else { SINGLE }, a, rm),
            14 => sf::convert(f, if fmt == 16 { DOUBLE } else { HALF }, a, rm),
            6..=9 => sf::to_int(f, a, op & 1 == 0, op < 8, rm),
            10..=13 => sf::from_int(f, a, op & 1 == 0, op < 12, rm),
            _ => panic!("bad operation"),
        };
        writeln!(out, "{v:016x} {flags}").unwrap();
    }
}
