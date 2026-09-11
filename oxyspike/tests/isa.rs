use oxyspike::{
    Cpu, Trap,
    memory::{Memory, RAM_BASE},
};
fn cpu(insn: u32) -> Cpu {
    let mut m = Memory::new(4096);
    m.store(RAM_BASE, 4, insn as u64).unwrap();
    Cpu::new(m, RAM_BASE)
}
fn r(f: u32, word: bool) -> u32 {
    0x02000000 | (2 << 20) | (1 << 15) | (f << 12) | (3 << 7) | if word { 0x3b } else { 0x33 }
}
#[test]
fn divide_corner_cases() {
    for word in [false, true] {
        for (a, b) in [
            (0, 0),
            (u64::MAX, 0),
            (0x8000000000000000, u64::MAX),
            (0x80000000, u64::MAX),
            (123, 7),
        ] {
            for f in 4..8 {
                let mut c = cpu(r(f, word));
                c.x[1] = a;
                c.x[2] = b;
                c.step().unwrap();
                let sa = if word {
                    a as i32 as i128
                } else {
                    a as i64 as i128
                };
                let sb = if word {
                    b as i32 as i128
                } else {
                    b as i64 as i128
                };
                let ua = if word { a as u32 as u64 } else { a };
                let ub = if word { b as u32 as u64 } else { b };
                let want = match f {
                    4 => {
                        if sb == 0 {
                            u64::MAX
                        } else {
                            (sa / sb) as u64
                        }
                    }
                    5 => ua.checked_div(ub).unwrap_or(u64::MAX),
                    6 => {
                        if sb == 0 {
                            sa as u64
                        } else {
                            (sa % sb) as u64
                        }
                    }
                    _ => {
                        if ub == 0 {
                            ua
                        } else {
                            ua % ub
                        }
                    }
                };
                assert_eq!(c.x[3], if word { want as i32 as u64 } else { want });
            }
        }
    }
}
#[test]
fn traps_do_not_retire_or_write_destination() {
    let mut c = cpu(0x0000b183);
    c.x[1] = 1;
    c.x[3] = 42;
    assert_eq!(c.step(), Err(Trap { cause: 5, value: 1 }));
    assert_eq!(c.pc, RAM_BASE);
    assert_eq!(c.retired, 0);
    assert_eq!(c.x[3], 42);
    c.memory.store(RAM_BASE, 4, 0xffffffff).unwrap();
    c.flush_instruction();
    assert_eq!(c.step().unwrap_err().cause, 2);
}
#[test]
fn compressed_fetch_at_ram_boundary() {
    let mut c = Cpu::new(Memory::new(2), RAM_BASE);
    c.memory.store(RAM_BASE, 2, 0x0085).unwrap();
    c.step().unwrap();
    assert_eq!(c.x[1], 1);
    assert_eq!(c.pc, RAM_BASE + 2);
}
#[test]
fn zero_register_and_word_sign_extension() {
    let mut c = cpu(0xfff00013);
    c.step().unwrap();
    assert_eq!(c.x[0], 0);
    let mut c = cpu(0x0010819b);
    c.x[1] = 0x7fffffff;
    c.step().unwrap();
    assert_eq!(c.x[3], 0xffffffff80000000);
}
#[test]
fn reservation_succeeds_once() {
    let mut c = cpu(0x1000b1af);
    c.x[1] = RAM_BASE + 128;
    c.memory.store(c.x[1], 8, 123).unwrap();
    c.step().unwrap();
    assert_eq!(c.x[3], 123);
    c.memory.store(RAM_BASE + 4, 4, 0x1820b22f).unwrap();
    c.x[2] = 456;
    c.step().unwrap();
    assert_eq!(c.x[4], 0);
    c.pc = RAM_BASE + 4;
    c.step().unwrap();
    assert_eq!(c.x[4], 1);
    assert_eq!(c.memory.load(c.x[1], 8).unwrap(), 456);
}
#[test]
fn htif_notification_tracks_only_stores_overlapping_tohost() {
    let mut m = Memory::new(4096);
    m.tohost = RAM_BASE + 128;
    m.store(RAM_BASE, 8, 1).unwrap();
    assert!(!m.htif_pending);
    m.store(RAM_BASE + 127, 1, 1).unwrap();
    assert!(!m.htif_pending);
    m.store(RAM_BASE + 136, 8, 1).unwrap();
    assert!(!m.htif_pending);
    for (offset, size) in [(128, 8), (129, 1), (135, 1), (127, 2), (124, 8)] {
        m.htif_pending = false;
        m.store(RAM_BASE + offset, size, 1).unwrap();
        assert!(m.htif_pending);
    }
    m.htif_pending = false;
    assert!(m.store(RAM_BASE + 4095, 8, 1).is_err());
    assert!(!m.htif_pending);
}
#[test]
fn clint_partial_msip_and_absent_harts_match_spike() {
    let mut m = Memory::new(4096);
    m.store(0x2000000, 1, 1).unwrap();
    for offset in 1..4 {
        m.interrupt_dirty = false;
        m.store(0x2000000 + offset, 1, 0).unwrap();
        assert_eq!(m.msip, 1);
        assert!(!m.interrupt_dirty);
    }
    assert_eq!(m.load(0x2000000, 8), Ok(1));
    m.store(0x2000000, 8, 0xffffffff00000000).unwrap();
    assert_eq!(m.msip, 0);
    for address in [0x2000004, 0x2003ff8, 0x2004008, 0x200bff0] {
        m.store(address, 8, u64::MAX).unwrap();
        assert_eq!(m.load(address, 8), Ok(0));
    }
    // The device splits even an unaligned double-word at the bank boundary.
    m.store(0x2004000, 8, u64::MAX).unwrap();
    m.store(0x2003ffc, 8, 0x0123456700000000).unwrap();
    assert_eq!(m.mtimecmp, 0xffffffff01234567);
    assert_eq!(m.load(0x2003ffc, 8), Ok(0x0123456700000000));
    assert_eq!(m.load(0x200c000, 8).unwrap_err().cause, 5);
}
#[test]
fn clint_timer_partial_writes_preserve_other_bytes_and_notify() {
    let mut m = Memory::new(4096);
    for address in [0x2004000, 0x200bff8] {
        m.store(address, 8, 0x0123456789abcdef).unwrap();
        m.interrupt_dirty = false;
        m.store(address + 4, 4, 0xdeadbeef).unwrap();
        assert!(m.interrupt_dirty);
        assert_eq!(m.load(address, 8), Ok(0xdeadbeef89abcdef));
        m.store(address + 1, 1, 0x12).unwrap();
        assert_eq!(m.load(address, 4), Ok(0x89ab12ef));
    }
}

#[test]
fn malformed_atomics_trap_before_address_checks() {
    for insn in [0x3002a32f, 0x1012a32f] {
        for address in [0, 1, RAM_BASE + 128] {
            let mut c = cpu(insn);
            c.x[5] = address;
            c.x[6] = 0x1234;
            let trap = c.step().unwrap_err();
            assert_eq!((trap.cause, trap.value), (2, insn as u64));
            assert_eq!(c.x[6], 0x1234);
            assert_eq!(c.retired, 0);
        }
    }
}
#[test]
fn lr_sc_reject_device_memory() {
    for (insn, cause, address) in [
        (0x1002a32f, 5, 0x2000000),
        (0x1802a32f, 7, 0x2000000),
        (0x1002b32f, 5, 0x2004000),
        (0x1802b32f, 7, 0x2004000),
    ] {
        let mut c = cpu(insn);
        c.x[5] = address;
        c.x[6] = 0x1234;
        let trap = c.step().unwrap_err();
        assert_eq!((trap.cause, trap.value), (cause, address));
        assert_eq!(c.x[6], 0x1234);
        assert_eq!(c.memory.msip, 0);
        assert_eq!(c.memory.mtimecmp, 0);
        assert_eq!(c.retired, 0);
    }
}
