use oxyspike::{
    elf,
    memory::{Memory, RAM_BASE},
};
fn put(b: &mut [u8], at: usize, size: usize, value: u64) {
    b[at..at + size].copy_from_slice(&value.to_le_bytes()[..size]);
}
fn header(size: usize) -> Vec<u8> {
    let mut b = vec![0; size];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put(&mut b, 18, 2, 243);
    put(&mut b, 24, 8, RAM_BASE);
    put(&mut b, 52, 2, 64);
    b
}
fn executable() -> Vec<u8> {
    let mut b = header(124);
    put(&mut b, 32, 8, 64);
    put(&mut b, 54, 2, 56);
    put(&mut b, 56, 2, 1);
    put(&mut b, 64, 4, 1);
    put(&mut b, 72, 8, 120);
    put(&mut b, 88, 8, RAM_BASE + 32);
    put(&mut b, 96, 8, 4);
    put(&mut b, 104, 8, 8);
    b[120..124].copy_from_slice(&[1, 2, 3, 4]);
    b
}
#[test]
fn segments_use_physical_addresses_and_clear_existing_bss() {
    let mut m = Memory::new(128);
    m.ram.fill(0xaa);
    let e = elf::load(&executable(), &mut m).unwrap();
    assert_eq!(e.entry, RAM_BASE);
    assert_eq!(&m.ram[32..40], &[1, 2, 3, 4, 0, 0, 0, 0]);
    assert_eq!(m.ram[31], 0xaa);
    assert_eq!(m.ram[40], 0xaa);
}
#[test]
fn every_truncation_of_a_segment_file_is_rejected() {
    let b = executable();
    for len in 0..b.len() {
        assert!(
            elf::load(&b[..len], &mut Memory::new(128)).is_err(),
            "len={len}"
        );
    }
}
#[test]
fn overflowing_tables_and_undersized_entries_are_rejected() {
    for (offset_field, stride_field, count_field, stride) in [(32, 54, 56, 56), (40, 58, 60, 64)] {
        for offset in [u64::MAX, u64::MAX - 4, u64::MAX - 63] {
            let mut b = header(64);
            put(&mut b, offset_field, 8, offset);
            put(&mut b, stride_field, 2, stride);
            put(&mut b, count_field, 2, 2);
            assert!(elf::load(&b, &mut Memory::new(128)).is_err());
        }
        let mut b = header(192);
        put(&mut b, offset_field, 8, 64);
        put(&mut b, stride_field, 2, stride - 1);
        put(&mut b, count_field, 2, 1);
        assert!(elf::load(&b, &mut Memory::new(128)).is_err());
    }
}
fn symbols() -> Vec<u8> {
    let mut b = header(224);
    put(&mut b, 40, 8, 64);
    put(&mut b, 58, 2, 64);
    put(&mut b, 60, 2, 2);
    put(&mut b, 68, 4, 2);
    put(&mut b, 88, 8, 192);
    put(&mut b, 96, 8, 24);
    put(&mut b, 104, 4, 1);
    put(&mut b, 120, 8, 24);
    put(&mut b, 132, 4, 3);
    put(&mut b, 152, 8, 216);
    put(&mut b, 160, 8, 8);
    put(&mut b, 200, 8, RAM_BASE + 64);
    b[216..223].copy_from_slice(b"tohost\0");
    b
}
#[test]
fn symbols_and_string_tables_are_bounded() {
    let b = symbols();
    assert_eq!(
        elf::load(&b, &mut Memory::new(128)).unwrap().tohost,
        Some(RAM_BASE + 64)
    );
    for (at, size, value) in [
        (88, 8, u64::MAX),
        (96, 8, u64::MAX),
        (96, 8, 23),
        (120, 8, 0),
        (104, 4, 2),
        (152, 8, u64::MAX),
        (160, 8, u64::MAX),
        (192, 4, 8),
    ] {
        let mut bad = b.clone();
        put(&mut bad, at, size, value);
        assert!(
            elf::load(&bad, &mut Memory::new(128)).is_err(),
            "field={at}"
        );
    }
    let mut bad = b;
    bad[216..224].fill(b'x');
    assert!(elf::load(&bad, &mut Memory::new(128)).is_err());
}
#[test]
fn oversized_or_overflowing_bss_is_rejected() {
    for (at, value) in [(104, u64::MAX), (104, 256), (88, u64::MAX), (104, 3)] {
        let mut b = executable();
        put(&mut b, at, 8, value);
        assert!(elf::load(&b, &mut Memory::new(128)).is_err());
    }
}
