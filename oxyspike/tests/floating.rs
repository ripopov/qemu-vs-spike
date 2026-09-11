use oxyspike::{
    Cpu,
    memory::{Memory, RAM_BASE},
    softfloat::{self as sf, DOUBLE, SINGLE},
};
#[test]
fn underflow_tininess_uses_unbounded_exponent_precision() {
    // Spike SoftFloat rounds to the smallest normal, but still sets UF + NX.
    assert_eq!(sf::mul(SINGLE, 0x00800000, 0x3f7fffff, 0), (0x00800000, 3));
    assert_eq!(
        sf::mul(DOUBLE, 0x0010000000000000, 0x3fefffffffffffff, 0),
        (0x0010000000000000, 3)
    );
}
#[test]
fn nan_signed_zero_and_comparison_flags() {
    assert_eq!(sf::add(DOUBLE, 0, 1 << 63, 2), (1 << 63, 0));
    assert_eq!(sf::add(DOUBLE, 0, 1 << 63, 0), (0, 0));
    assert_eq!(sf::minmax(DOUBLE, 0, 1 << 63, false), (1 << 63, 0));
    assert_eq!(sf::minmax(DOUBLE, 0, 1 << 63, true), (0, 0));
    assert_eq!(sf::minmax(DOUBLE, 0x7ff0000000000001, 0, false), (0, 16));
    assert_eq!(sf::compare(SINGLE, 0x7fc00000, 0, 2), (0, 0));
    assert_eq!(sf::compare(SINGLE, 0x7fc00000, 0, 1), (0, 16));
    assert_eq!(
        sf::fma(SINGLE, 0, 0x7f800000, 0x7fc00000, 0),
        (0x7fc00000, 16)
    );
}
fn fp_cpu(i: u32) -> Cpu {
    let mut c = Cpu::new(Memory::new(4096), RAM_BASE);
    c.csr_write(0x300, 1 << 13);
    c.memory.store(RAM_BASE, 4, i as u64).unwrap();
    c
}
#[test]
fn dynamic_rounding_and_reserved_modes() {
    // FADD.S f3,f1,f2, dyn: halfway above 1.0.
    let mut c = fp_cpu(0x0020f1d3);
    c.f[1] = 0xffffffff3f800000;
    c.f[2] = 0xffffffff33800000;
    c.csr_write(2, 3);
    c.step().unwrap();
    assert_eq!(c.f[3], 0xffffffff3f800001);
    assert_eq!(c.csrs[3] & 31, 1);
    c.pc = RAM_BASE;
    c.csr_write(2, 5);
    let before = c.f[3];
    assert_eq!(c.step().unwrap_err().cause, 2);
    assert_eq!(c.f[3], before);
}
#[test]
fn bad_nan_box_is_quiet_nan_but_moves_preserve_raw_payload() {
    let mut c = fp_cpu(0xe00091d3); // FCLASS.S x3,f1
    c.f[1] = 0x3f800000;
    c.step().unwrap();
    assert_eq!(c.x[3], 1 << 9);
    assert_eq!(c.csrs[3], 0);
    c.pc = RAM_BASE;
    c.memory.store(RAM_BASE, 4, 0xe00081d3).unwrap();
    c.flush_instruction(); // FMV.X.W x3,f1
    c.step().unwrap();
    assert_eq!(c.x[3], 0x3f800000);
}
#[test]
fn single_double_conversion_and_negative_fma_encodings() {
    let mut c = fp_cpu(0x420081d3);
    c.f[1] = 0xffffffff3f800000; // FCVT.D.S f3,f1
    c.step().unwrap();
    assert_eq!(c.f[3], 0x3ff0000000000000);
    for (op, want) in [(0x43, 11.0f64), (0x47, 1.0), (0x4b, -1.0), (0x4f, -11.0)] {
        let i = (3 << 27) | (1 << 25) | (2 << 20) | (1 << 15) | (4 << 7) | op;
        let mut c = fp_cpu(i);
        c.f[1] = 2.0f64.to_bits();
        c.f[2] = 3.0f64.to_bits();
        c.f[3] = 5.0f64.to_bits();
        c.step().unwrap();
        assert_eq!(c.f[4], want.to_bits());
    }
}
#[test]
fn zfhmin_rejects_full_half_arithmetic_and_reserved_conversions() {
    for i in [0x042081d3, 0xe40091d3, 0xc40081d3, 0x442081d3, 0x042081c3] {
        let mut c = fp_cpu(i); // FADD.H, FCLASS.H, FCVT.W.H, H-to-H, half FMA.
        assert_eq!(c.step().unwrap_err().cause, 2, "{i:08x}");
    }
}
#[test]
fn half_rounding_flags_and_nan_boxing() {
    let mut c = fp_cpu(0x4400f1d3); // FCVT.H.S f3,f1,dyn
    c.f[1] = 0xffffffff3f801000; // Halfway between 1 and next half value.
    c.csrs[3] = 3 << 5; // RUP.
    c.step().unwrap();
    assert_eq!(c.f[3], 0xffffffffffff3c01);
    assert_eq!(c.csrs[3] & 31, 1);
    let mut c = fp_cpu(0x402081d3); // FCVT.S.H f3,f1,RNE
    c.f[1] = 0xffffffffffff7c01; // Signaling half NaN.
    c.step().unwrap();
    assert_eq!(c.f[3], 0xffffffff7fc00000);
    assert_eq!(c.csrs[3] & 31, 16);
    let mut c = fp_cpu(0x4400f1d3);
    c.csrs[3] = 5 << 5;
    assert_eq!(c.step().unwrap_err().cause, 2);
}
