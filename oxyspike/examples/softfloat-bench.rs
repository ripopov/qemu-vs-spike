//! Diagnostic arithmetic-only benchmark; not a guest or CoreMark score.
use oxyspike::softfloat::{self, DOUBLE, SINGLE};
use std::{hint::black_box, time::Instant};
fn main() {
    let op = std::env::args()
        .nth(1)
        .expect("operation: add, mul, div, fma, convert");
    let iterations = 100_000usize;
    let exponents = [0u64, 1, 2, 511, 1022, 1023, 1024, 1535, 2045, 2046];
    let mut state = 0x471e_c357_b184_02adu64;
    let mut inputs = [0; 256];
    for (i, x) in inputs.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *x = (state & 0x800f_ffff_ffff_ffff) | (exponents[i % exponents.len()] << 52);
    }
    let start = Instant::now();
    let mut checksum = 0u64;
    for i in 0..iterations {
        let a = black_box(inputs[i & 255]);
        let b = black_box(inputs[(i * 73 + 19) & 255]);
        let c = black_box(inputs[(i * 31 + 7) & 255]);
        let rm = (i % 5) as u32;
        let (bits, flags) = match op.as_str() {
            "add" => softfloat::add(DOUBLE, a, b, rm),
            "mul" => softfloat::mul(DOUBLE, a, b, rm),
            "div" => softfloat::div(DOUBLE, a, b, rm),
            "fma" => softfloat::fma(DOUBLE, a, b, c, rm),
            "convert" => softfloat::convert(DOUBLE, SINGLE, a, rm),
            _ => panic!("unknown operation"),
        };
        checksum = checksum.rotate_left(1) ^ bits ^ flags as u64;
    }
    black_box(checksum);
    println!(
        "{{\"operation\":\"{op}\",\"iterations\":{iterations},\"seconds\":{},\"checksum\":{checksum}}}",
        start.elapsed().as_secs_f64()
    );
}
