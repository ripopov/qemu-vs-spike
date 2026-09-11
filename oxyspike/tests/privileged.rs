use oxyspike::{
    Cpu, Trap,
    memory::{Memory, RAM_BASE},
};
fn cpu() -> Cpu {
    Cpu::new(Memory::new(65536), RAM_BASE)
}
fn all_memory(c: &mut Cpu) {
    c.csr_write(0x3b0, u64::MAX);
    c.csr_write(0x3a0, 0x1f);
}
#[test]
fn delegated_trap_and_sret_restore_interrupt_state() {
    let mut c = cpu();
    all_memory(&mut c);
    c.privilege = 0;
    c.csr_write(0x302, 1 << 8);
    c.csr_write(0x105, RAM_BASE + 256);
    c.csr_write(0x300, 2);
    c.take_trap(Trap { cause: 8, value: 0 });
    assert_eq!(c.privilege, 1);
    assert_eq!(c.pc, RAM_BASE + 256);
    assert_eq!(c.csr_read(0x141), Some(RAM_BASE));
    assert_eq!(c.csr_read(0x100).unwrap() & 0x122, 0x20);
    c.memory.store(c.pc, 4, 0x10200073).unwrap();
    c.step().unwrap();
    assert_eq!(c.privilege, 0);
    assert_eq!(c.pc, RAM_BASE);
    assert_eq!(c.csr_read(0x100).unwrap() & 0x122, 0x22);
}
#[test]
fn machine_return_clears_mprv_below_machine() {
    let mut c = cpu();
    c.memory.store(c.pc, 4, 0x30200073).unwrap();
    c.csr_write(0x341, RAM_BASE + 8);
    c.csr_write(0x300, (1 << 17) | (1 << 11) | (1 << 7));
    c.step().unwrap();
    assert_eq!(c.privilege, 1);
    assert_eq!(c.pc, RAM_BASE + 8);
    assert_eq!(
        c.csrs[0x300] & ((1 << 17) | (3 << 11) | (1 << 3) | (1 << 7)),
        0x88
    );
}
#[test]
fn readonly_and_privilege_csr_faults() {
    let mut c = cpu();
    c.memory.store(c.pc, 4, 0xf1101073).unwrap();
    assert_eq!(c.step().unwrap_err().cause, 2);
    all_memory(&mut c);
    c.privilege = 1;
    c.memory.store(c.pc, 4, 0x300020f3).unwrap();
    c.flush_instruction();
    assert_eq!(c.step().unwrap_err().cause, 2);
}
#[test]
fn timer_interrupt_priority_and_delegation() {
    let mut c = cpu();
    c.memory.mtime = 10;
    c.memory.mtimecmp = 10;
    c.csr_write(0x304, 128);
    c.csr_write(0x305, RAM_BASE + 257);
    assert!(!c.interrupt());
    c.privilege = 1;
    assert!(c.interrupt());
    assert_eq!(c.pc, RAM_BASE + 256 + 28);
    assert_eq!(c.csrs[0x342], (1 << 63) | 7);
}
#[test]
fn pmp_tor_partial_match_and_lock() {
    let mut c = cpu();
    c.csr_write(0x3b0, RAM_BASE >> 2);
    c.csr_write(0x3b1, (RAM_BASE + 4096) >> 2);
    c.csr_write(0x3a0, 0x8900); // Locked read-only TOR.
    assert_eq!(c.translate(RAM_BASE, 8, 1), Ok(RAM_BASE));
    assert_eq!(c.translate(RAM_BASE, 8, 2).unwrap_err().cause, 7);
    assert_eq!(c.translate(RAM_BASE + 4092, 8, 1).unwrap_err().cause, 5);
    c.csr_write(0x3b0, 0);
    assert_eq!(c.csr_read(0x3b0), Some(RAM_BASE >> 2));
    c.csr_write(0x3a0, 0);
    assert_eq!(c.csr_read(0x3a0), Some(0x8900));
}
fn paged() -> Cpu {
    let mut c = cpu();
    all_memory(&mut c);
    c.memory
        .store(RAM_BASE, 8, ((RAM_BASE + 4096) >> 12) << 10 | 1)
        .unwrap();
    c.memory
        .store(RAM_BASE + 4096, 8, ((RAM_BASE + 8192) >> 12) << 10 | 1)
        .unwrap();
    c.memory
        .store(
            RAM_BASE + 8192 + 32,
            8,
            ((RAM_BASE + 12288) >> 12) << 10 | 0xcf,
        )
        .unwrap();
    c.csr_write(0x180, (8 << 60) | (RAM_BASE >> 12));
    c.privilege = 1;
    c
}
#[test]
fn sv39_permissions_canonicality_and_ad_faults() {
    let mut c = paged();
    assert_eq!(c.translate(0x4000, 8, 0), Ok(RAM_BASE + 12288));
    assert_eq!(c.translate(0x4000, 8, 2), Ok(RAM_BASE + 12288));
    assert_eq!(c.translate(1 << 39 | 0x4000, 8, 1).unwrap_err().cause, 13);
    let pte = c.memory.load(RAM_BASE + 8224, 8).unwrap();
    c.memory.store(RAM_BASE + 8224, 8, pte & !128).unwrap();
    assert_eq!(c.translate(0x4000, 8, 2).unwrap_err().cause, 15);
    c.memory.store(RAM_BASE + 8224, 8, pte | 16).unwrap();
    assert_eq!(c.translate(0x4000, 8, 1).unwrap_err().cause, 13);
    c.csr_write(0x300, 1 << 18);
    assert!(c.translate(0x4000, 8, 1).is_ok());
    assert_eq!(c.translate(0x4000, 8, 0).unwrap_err().cause, 12);
}
#[test]
fn fetch_preserves_page_fault_cause_and_address() {
    let mut c = paged();
    c.pc = 0x5000;
    assert_eq!(
        c.step(),
        Err(Trap {
            cause: 12,
            value: 0x5000
        })
    );
    assert_eq!(c.retired, 0);
}
#[test]
fn firmware_can_forward_timer_interrupt_to_supervisor() {
    let mut c = cpu();
    c.privilege = 1;
    c.csr_write(0x303, 1 << 5);
    c.csr_write(0x304, 1 << 5);
    c.csr_write(0x300, 2);
    c.csr_write(0x105, RAM_BASE + 128);
    c.csr_write(0x344, 1 << 5);
    assert!(c.interrupt());
    assert_eq!(c.privilege, 1);
    assert_eq!(c.csrs[0x142], (1 << 63) | 5);
    assert_eq!(c.pc, RAM_BASE + 128);
    c.csr_write(0x344, 0);
    assert_eq!(c.csr_read(0x144), Some(0));
}
#[test]
fn translation_cache_rechecks_permissions_and_pmp_updates() {
    let mut c = paged();
    c.memory.store(RAM_BASE + 12288, 8, 123).unwrap();
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(123));
    c.privilege = 0;
    assert_eq!(c.load_virtual(0x4000, 8, 1).unwrap_err().cause, 13);
    c.privilege = 3;
    c.csr_write(0x300, (1 << 17) | (1 << 11));
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(123));
    c.csr_write(0x300, 1 << 17);
    assert_eq!(c.load_virtual(0x4000, 8, 1).unwrap_err().cause, 13);
    c.privilege = 1;
    c.csr_write(0x3a0, 0);
    assert_eq!(c.load_virtual(0x4000, 8, 1).unwrap_err().cause, 5);
}
#[test]
fn sfence_exposes_modified_page_table_after_cached_access() {
    let mut c = paged();
    c.memory.store(RAM_BASE + 12288, 8, 123).unwrap();
    c.memory.store(RAM_BASE + 16384, 8, 456).unwrap();
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(123));
    c.memory
        .store(RAM_BASE + 8224, 8, ((RAM_BASE + 16384) >> 12) << 10 | 0xcf)
        .unwrap();
    // Execute the fence from M-mode; it must also invalidate S-mode translations.
    c.privilege = 3;
    c.pc = RAM_BASE + 20000;
    c.memory.store(c.pc, 4, 0x12000073).unwrap();
    c.step().unwrap();
    c.privilege = 1;
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(456));
}
#[test]
fn cached_read_does_not_grant_write_and_sum_does_not_grant_execute() {
    let mut c = paged();
    let pte = c.memory.load(RAM_BASE + 8224, 8).unwrap();
    c.memory.store(RAM_BASE + 8224, 8, pte & !4).unwrap();
    assert!(c.load_virtual(0x4000, 8, 1).is_ok());
    assert_eq!(c.store_virtual(0x4000, 8, 1).unwrap_err().cause, 15);
    c.memory.store(RAM_BASE + 8224, 8, pte | 16).unwrap();
    c.flush_translation();
    c.csr_write(0x300, 1 << 18);
    assert!(c.load_virtual(0x4000, 8, 1).is_ok());
    assert_eq!(c.load_virtual(0x4000, 2, 0).unwrap_err().cause, 12);
    c.csr_write(0x300, 0);
    assert_eq!(c.load_virtual(0x4000, 8, 1).unwrap_err().cause, 13);
}
#[test]
fn subpage_pmp_boundary_cannot_be_hidden_by_cache() {
    let mut c = cpu();
    c.csr_write(0x3b0, RAM_BASE >> 2);
    c.csr_write(0x3b1, (RAM_BASE + 16) >> 2);
    c.csr_write(0x3a0, 0x0900);
    c.privilege = 1;
    assert!(c.load_virtual(RAM_BASE, 8, 1).is_ok());
    assert_eq!(c.load_virtual(RAM_BASE + 16, 8, 1).unwrap_err().cause, 5);
}
#[test]
fn instruction_cache_fence_and_privilege_tags() {
    let mut c = cpu();
    all_memory(&mut c);
    c.memory.store(RAM_BASE, 4, 0x00100093).unwrap();
    c.step().unwrap();
    assert_eq!(c.x[1], 1);
    c.memory.store(RAM_BASE, 4, 0x00200093).unwrap();
    c.memory.store(RAM_BASE + 4, 4, 0x0000100f).unwrap();
    c.step().unwrap(); // FENCE.I
    c.pc = RAM_BASE;
    c.step().unwrap();
    assert_eq!(c.x[1], 2);
    c.csr_write(0x3a0, 0);
    c.privilege = 1;
    c.pc = RAM_BASE;
    assert_eq!(c.step().unwrap_err().cause, 1);
}
#[test]
fn instruction_cache_does_not_hide_remapped_page_or_second_half_fault() {
    let mut c = paged();
    c.memory.store(RAM_BASE + 12288, 4, 0x00100093).unwrap();
    c.pc = 0x4000;
    c.step().unwrap();
    c.memory.store(RAM_BASE + 16384, 4, 0x00200093).unwrap();
    c.memory
        .store(RAM_BASE + 8224, 8, ((RAM_BASE + 16384) >> 12) << 10 | 0xcf)
        .unwrap();
    c.flush_translation();
    c.pc = 0x4000;
    c.step().unwrap();
    assert_eq!(c.x[1], 2);
    c.memory.store(RAM_BASE + 16384 + 4094, 2, 0x0093).unwrap();
    c.pc = 0x4ffe;
    assert_eq!(
        c.step(),
        Err(Trap {
            cause: 12,
            value: 0x5000
        })
    );
    assert_eq!(c.pc, 0x4ffe);
}
#[test]
fn illegal_compressed_fp_reports_original_encoding() {
    let mut c = cpu();
    c.memory.store(RAM_BASE, 2, 0x2000).unwrap(); // C.FLD with FS=Off.
    assert_eq!(
        c.step(),
        Err(Trap {
            cause: 2,
            value: 0x2000
        })
    );
}
#[test]
fn counter_writes_and_inhibit_use_pre_instruction_state() {
    let mut c = cpu();
    c.x[1] = 5;
    c.memory.store(RAM_BASE, 4, 0x32009073).unwrap(); // CSRW mcountinhibit,x1
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (1, 1));
    c.memory.store(RAM_BASE + 4, 4, 0x00000013).unwrap();
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (1, 1));
    c.memory.store(RAM_BASE + 8, 4, 0x32001073).unwrap();
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (1, 1));
    c.memory.store(RAM_BASE + 12, 4, 0x00000013).unwrap();
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (2, 2));
    c.x[1] = 100;
    c.memory.store(RAM_BASE + 16, 4, 0xb0209073).unwrap();
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (3, 100));
    c.memory.store(RAM_BASE + 20, 4, 0xb0009073).unwrap();
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (100, 101));
}
#[test]
fn counter_independent_inhibition_wraparound_and_faults() {
    let mut c = cpu();
    c.memory.store(RAM_BASE, 4, 0x00000013).unwrap();
    c.csr_write(0xb00, u64::MAX);
    c.csr_write(0xb02, u64::MAX);
    c.step().unwrap();
    assert_eq!((c.cycles, c.retired), (0, 0));
    for (mask, want) in [(1, (0, 1)), (4, (1, 0)), (5, (0, 0))] {
        c.pc = RAM_BASE;
        c.cycles = 0;
        c.retired = 0;
        c.csr_write(0x320, mask);
        c.step().unwrap();
        assert_eq!((c.cycles, c.retired), want);
    }
    c.csr_write(0x320, 0);
    c.pc = RAM_BASE + 4096;
    c.cycles = 7;
    c.retired = 8;
    assert!(c.step().is_err());
    assert_eq!((c.cycles, c.retired), (7, 8));
}
#[test]
fn pbmt_leaf_attributes_require_pbmte_and_reject_reserved_values() {
    let mut c = paged();
    let base = c.memory.load(RAM_BASE + 8224, 8).unwrap();
    for attr in 1..=2 {
        c.memory
            .store(RAM_BASE + 8224, 8, base | (attr << 61))
            .unwrap();
        assert_eq!(c.translate(0x4000, 8, 1).unwrap_err().cause, 13);
        c.csr_write(0x30a, 1 << 62);
        assert_eq!(c.translate(0x4000, 8, 1), Ok(RAM_BASE + 12288));
        assert!(c.load_virtual(0x4000, 8, 1).is_ok());
        c.csr_write(0x30a, 0);
        assert_eq!(c.load_virtual(0x4000, 8, 1).unwrap_err().cause, 13);
    }
    c.csr_write(0x30a, 1 << 62);
    c.memory
        .store(RAM_BASE + 8224, 8, base | (3 << 61))
        .unwrap();
    assert_eq!(c.translate(0x4000, 8, 1).unwrap_err().cause, 13);
}
#[test]
fn pbmt_nonleaf_and_other_reserved_pte_bits_fault() {
    let mut c = paged();
    c.csr_write(0x30a, 1 << 62);
    let root = c.memory.load(RAM_BASE, 8).unwrap();
    c.memory.store(RAM_BASE, 8, root | (1 << 61)).unwrap();
    assert_eq!(c.translate(0x4000, 2, 0).unwrap_err().cause, 12);
    c.memory.store(RAM_BASE, 8, root).unwrap();
    let leaf = c.memory.load(RAM_BASE + 8224, 8).unwrap();
    for bit in [54, 55, 56, 57, 58, 59, 60, 63] {
        c.memory
            .store(RAM_BASE + 8224, 8, leaf | (1 << bit))
            .unwrap();
        assert_eq!(c.translate(0x4000, 8, 2).unwrap_err().cause, 15);
    }
}
#[test]
fn envcfg_only_exposes_implemented_fields() {
    let mut c = cpu();
    c.csr_write(0x30a, u64::MAX);
    assert_eq!(c.csr_read(0x30a), Some((1 << 62) | 0xf1));
    c.csr_write(0x10a, u64::MAX);
    assert_eq!(c.csr_read(0x10a), Some(0xf1));
    c.csr_write(0x30a, 0x20);
    assert_eq!(c.csr_read(0x30a), Some(0));
}
#[test]
fn sinval_refreshes_cached_translation_and_checks_privilege() {
    let mut c = paged();
    c.memory.store(RAM_BASE + 12288, 8, 123).unwrap();
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(123));
    c.memory.store(RAM_BASE + 16384, 8, 456).unwrap();
    c.memory
        .store(RAM_BASE + 8224, 8, ((RAM_BASE + 16384) >> 12) << 10 | 0xcf)
        .unwrap();
    c.privilege = 3;
    c.pc = RAM_BASE + 20000;
    c.memory.store(c.pc, 4, 0x16000073).unwrap();
    c.step().unwrap();
    c.privilege = 1;
    assert_eq!(c.load_virtual(0x4000, 8, 1), Ok(456));
    c.privilege = 0;
    c.pc = 0x4000;
    c.flush_instruction();
    c.memory.store(RAM_BASE + 16384, 4, 0x16000073).unwrap();
    // User mapping with execute permission, so the instruction itself must trap.
    let pte = c.memory.load(RAM_BASE + 8224, 8).unwrap();
    c.memory.store(RAM_BASE + 8224, 8, pte | 16).unwrap();
    c.flush_translation();
    assert_eq!(c.step().unwrap_err().cause, 2);
}
#[test]
fn sinval_tvm_and_ordering_fences_match_supervisor_rules() {
    let mut c = cpu();
    all_memory(&mut c);
    c.privilege = 1;
    c.csr_write(0x300, 1 << 20);
    for (insn, ok) in [(0x16000073, false), (0x18000073, true), (0x18100073, true)] {
        c.pc = RAM_BASE;
        c.memory.store(c.pc, 4, insn).unwrap();
        c.flush_instruction();
        assert_eq!(c.step().is_ok(), ok);
    }
}
#[test]
fn event_interrupt_poll_matches_eager_checks() {
    let mut eager = cpu();
    let mut event = cpu();
    for c in [&mut eager, &mut event] {
        all_memory(c);
        c.csr_write(0x305, RAM_BASE + 256);
        c.csr_write(0x105, RAM_BASE + 512);
        c.memory.store(RAM_BASE + 256, 4, 0x30200073).unwrap();
        c.memory.store(RAM_BASE + 512, 4, 0x10200073).unwrap();
    }
    let mut seed = 0x76543210abcdef01u64;
    for iteration in 0..4000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        for c in [&mut eager, &mut event] {
            match iteration % 14 {
                0 => c.csr_write(0x304, seed & 0xaaa),
                1 => c.csr_write(0x300, seed & 0x19aa),
                2 => c.csr_write(0x344, seed & 0x222),
                3 => c.csr_write(0x303, seed & 0x222),
                4 => c.memory.store(0x2000000, 4, seed & 1).unwrap(),
                5 => c.memory.store(0x2004000, 8, seed % 100).unwrap(),
                6 => c.memory.store(0x200bff8, 8, seed % 100).unwrap(),
                7 => c.memory.advance_time(1),
                8 => c.csr_write(0x100, seed & 0x122),
                9 => c.csr_write(0x104, seed & 0x222),
                10 => c.csr_write(0x144, seed & 2),
                11 => {
                    c.take_trap(Trap { cause: 8, value: 0 });
                    c.step().unwrap(); // MRET/SRET changes eligibility.
                }
                _ => {} // Unchanged state must not require another full check.
            }
        }
        for _ in 0..3 {
            assert_eq!(
                eager.interrupt(),
                event.poll_interrupt(),
                "event {iteration}"
            );
            assert_eq!(eager.pc, event.pc);
            assert_eq!(eager.privilege, event.privilege);
            assert_eq!(eager.csrs, event.csrs);
        }
    }
}
#[test]
fn event_poll_rechecks_pending_interrupt_after_each_trap_return() {
    for supervisor in [false, true] {
        let mut c = cpu();
        all_memory(&mut c);
        c.memory
            .store(
                RAM_BASE,
                4,
                if supervisor { 0x10200073 } else { 0x30200073 },
            )
            .unwrap();
        c.csr_write(0x305, RAM_BASE + 256);
        c.csr_write(0x105, RAM_BASE + 512);
        if supervisor {
            c.csr_write(0x303, 2);
            c.csr_write(0x304, 2);
            c.csr_write(0x344, 2);
            c.csr_write(0x300, 32 | 256); // SPIE=1, SIE=0, return to S.
            c.csr_write(0x141, RAM_BASE + 4);
            c.privilege = 1;
        } else {
            c.csr_write(0x304, 8);
            c.memory.store(0x2000000, 4, 1).unwrap();
            c.csr_write(0x300, 128 | (3 << 11)); // MPIE=1, MIE=0, return to M.
            c.csr_write(0x341, RAM_BASE + 4);
        }
        assert!(!c.poll_interrupt());
        assert!(!c.memory.interrupt_dirty);
        c.step().unwrap();
        assert!(c.poll_interrupt());
        assert_eq!(c.pc, RAM_BASE + if supervisor { 512 } else { 256 });
    }
}
#[test]
fn reset_timer_is_pending_but_disabled_and_rearms_after_compare_write() {
    let mut c = cpu();
    assert_eq!(c.memory.mtimecmp, 0);
    assert_eq!(c.csr_read(0x344).unwrap() & 128, 128);
    assert!(!c.poll_interrupt()); // MIE/mie are disabled at reset.
    c.memory.store(0x2004000, 8, 5).unwrap();
    assert!(!c.poll_interrupt());
    assert_eq!(c.csr_read(0x344).unwrap() & 128, 0);
    c.memory.advance_time(5);
    assert!(!c.poll_interrupt());
    assert_eq!(c.csr_read(0x344).unwrap() & 128, 128);
    c.csr_write(0x305, RAM_BASE + 256);
    c.csr_write(0x304, 128);
    c.csr_write(0x300, 8);
    assert!(c.poll_interrupt());
    assert_eq!(c.pc, RAM_BASE + 256);
}

#[test]
fn instruction_cache_observes_address_space_switches_through_both_csr_interfaces() {
    let mut c = paged();
    let first = c.csrs[0x180];
    let second_root = RAM_BASE + 20480;
    let second = (8 << 60) | (second_root >> 12);
    c.memory.store(RAM_BASE + 12288, 4, 0x00100093).unwrap();
    c.memory
        .store(second_root, 8, ((RAM_BASE + 24576) >> 12) << 10 | 1)
        .unwrap();
    c.memory
        .store(RAM_BASE + 24576, 8, ((RAM_BASE + 28672) >> 12) << 10 | 1)
        .unwrap();
    c.memory
        .store(RAM_BASE + 28704, 8, ((RAM_BASE + 32768) >> 12) << 10 | 0xcf)
        .unwrap();
    c.memory.store(RAM_BASE + 32768, 4, 0x00200093).unwrap();
    for direct in [false, true] {
        for (satp, value) in [(first, 1), (second, 2), (first, 1), (second, 2)] {
            if direct {
                c.csrs[0x180] = satp;
            } else {
                c.csr_write(0x180, satp);
            }
            for _ in 0..2 {
                c.pc = 0x4000;
                c.step().unwrap();
                assert_eq!(c.x[1], value);
            }
        }
    }
}

#[test]
fn split_store_commits_first_page_before_second_page_fault() {
    let value = 0x1122334455667788u64;
    for first_bytes in 1..8 {
        for second_mapped in [false, true] {
            let mut c = paged();
            if second_mapped {
                c.memory
                    .store(RAM_BASE + 8232, 8, ((RAM_BASE + 16384) >> 12) << 10 | 0xcf)
                    .unwrap();
            }
            let va = 0x5000 - first_bytes;
            let result = c.store_virtual(va, 8, value);
            if second_mapped {
                result.unwrap();
                assert_eq!(c.load_virtual(va, 8, 1).unwrap(), value);
            } else {
                assert_eq!(
                    result,
                    Err(Trap {
                        cause: 15,
                        value: 0x5000
                    })
                );
            }
            for n in 0..first_bytes {
                assert_eq!(
                    c.memory
                        .load(RAM_BASE + 16384 - first_bytes + n, 1)
                        .unwrap(),
                    (value >> (n * 8)) & 255
                );
            }
        }
    }
}
#[test]
fn split_store_validates_whole_first_fragment_before_writing() {
    let mut c = paged();
    // Deny the last four physical bytes of the first mapped page; permit the rest.
    c.csr_write(0x3b0, (RAM_BASE + 16380) >> 2);
    c.csr_write(0x3b1, u64::MAX);
    c.csr_write(0x3a0, 0x1f10);
    assert_eq!(
        c.store_virtual(0x4ff9, 8, u64::MAX),
        Err(Trap {
            cause: 7,
            value: 0x4ff9
        })
    );
    assert_eq!(&c.memory.ram[16377..16384], &[0; 7]);
}

#[test]
fn split_load_checks_whole_fragment_and_reports_its_start() {
    let mut c = paged();
    c.csr_write(0x3b0, (RAM_BASE + 16380) >> 2);
    c.csr_write(0x3b1, u64::MAX);
    c.csr_write(0x3a0, 0x1f10);
    assert_eq!(
        c.load_virtual(0x4ff9, 8, 1),
        Err(Trap {
            cause: 5,
            value: 0x4ff9
        })
    );
}
#[test]
fn split_load_handles_all_fragment_lengths_and_unmapped_tails() {
    for first_bytes in 1..8 {
        let mut c = paged();
        for n in 0..first_bytes {
            c.memory
                .store(RAM_BASE + 16384 - first_bytes + n, 1, n + 1)
                .unwrap();
        }
        let va = 0x5000 - first_bytes;
        assert_eq!(
            c.load_virtual(va, 8, 1),
            Err(Trap {
                cause: 13,
                value: 0x5000
            })
        );
        c.memory
            .store(RAM_BASE + 8232, 8, ((RAM_BASE + 16384) >> 12) << 10 | 0xcf)
            .unwrap();
        for n in first_bytes..8 {
            c.memory
                .store(RAM_BASE + 16384 + n - first_bytes, 1, n + 1)
                .unwrap();
        }
        c.flush_translation();
        for _ in 0..2 {
            assert_eq!(c.load_virtual(va, 8, 1).unwrap(), 0x0807060504030201);
        }
    }
}

#[test]
fn hpm_counters_are_zero_and_permissions_follow_both_enable_masks() {
    for csr in 0xc03..=0xc1f {
        for privilege in [0, 1, 3] {
            for machine_enabled in [false, true] {
                for supervisor_enabled in [false, true] {
                    let mut c = cpu();
                    all_memory(&mut c);
                    let bit = 1 << (csr - 0xc00);
                    c.csr_write(0x306, if machine_enabled { bit } else { 0 });
                    c.csr_write(0x106, if supervisor_enabled { bit } else { 0 });
                    c.privilege = privilege;
                    let instruction = ((csr as u64) << 20) | 0x22f3; // csrr t0, csr
                    c.memory.store(c.pc, 4, instruction).unwrap();
                    let result = c.step();
                    let allowed =
                        privilege == 3 || machine_enabled && (privilege == 1 || supervisor_enabled);
                    if allowed {
                        result.unwrap();
                        assert_eq!(c.x[5], 0);
                    } else {
                        assert_eq!(
                            result,
                            Err(Trap {
                                cause: 2,
                                value: instruction
                            })
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn hpm_machine_writes_are_ignored_and_user_aliases_are_readonly() {
    let mut c = cpu();
    for csr in 0xb03..=0xb1f {
        c.csr_write(csr, u64::MAX);
        assert_eq!(c.csr_read(csr), Some(0));
        c.csr_write(csr - 0x7e0, u64::MAX);
        assert_eq!(c.csr_read(csr - 0x7e0), Some(0));
        let instruction = (((csr + 0x100) as u64) << 20) | 0x1073;
        c.memory.store(c.pc, 4, instruction).unwrap();
        c.flush_instruction();
        assert_eq!(
            c.step(),
            Err(Trap {
                cause: 2,
                value: instruction
            })
        );
    }
    for csr in [0x106, 0x306, 0x320] {
        c.csr_write(csr, u64::MAX);
        assert_eq!(
            c.csr_read(csr),
            Some(if csr == 0x320 {
                0xffff_fffd
            } else {
                0xffff_ffff
            })
        );
    }
    for csr in [0xb83, 0xb9f, 0xc83, 0xc9f, 0x723, 0x73f] {
        assert_eq!(c.csr_read(csr), None); // RV32 high halves do not exist in RV64.
    }
}

#[test]
fn cache_blocks_use_load_permissions_and_preserve_original_fault_addresses() {
    for (flags, mxr, clean_ok, zero_ok) in [
        (0x43, false, true, false), // Readable, accessed, neither writable nor dirty.
        (0x47, false, true, false), // Writable but not dirty.
        (0xc7, false, true, true),
        (0x49, false, false, false), // Execute-only.
        (0x49, true, true, false),
        (0x03, false, false, false), // Accessed bit absent.
    ] {
        for op in [0, 1, 2, 4] {
            let mut c = paged();
            c.privilege = 3;
            c.csr_write(0x300, (1 << 17) | (1 << 11) | if mxr { 1 << 19 } else { 0 });
            c.memory
                .store(RAM_BASE + 8224, 8, ((RAM_BASE + 12288) >> 12) << 10 | flags)
                .unwrap();
            c.pc = RAM_BASE + 16384;
            c.x[5] = 0x4007;
            c.memory.store(c.pc, 4, (op << 20) | 0x2a00f).unwrap();
            let result = c.step();
            if if op == 4 { zero_ok } else { clean_ok } {
                result.unwrap();
            } else {
                assert_eq!(
                    result,
                    Err(Trap {
                        cause: 15,
                        value: 0x4007
                    })
                );
            }
        }
    }
}

#[test]
fn cache_block_envcfg_checks_precede_address_translation() {
    for privilege in [0, 1, 3] {
        for menabled in [false, true] {
            for senabled in [false, true] {
                for (op, mask) in [(0, 0x10), (1, 0x40), (2, 0x40), (4, 0x80)] {
                    let mut c = cpu();
                    all_memory(&mut c);
                    c.csr_write(0x30a, if menabled { mask } else { 0 });
                    c.csr_write(0x10a, if senabled { mask } else { 0 });
                    c.privilege = privilege;
                    c.x[5] = 0x10000007; // Unmapped physical memory.
                    let instruction = (op << 20) | 0x2a00f;
                    c.memory.store(c.pc, 4, instruction).unwrap();
                    let allowed = privilege == 3 || menabled && (privilege == 1 || senabled);
                    assert_eq!(
                        c.step(),
                        Err(if allowed {
                            Trap {
                                cause: 7,
                                value: c.x[5],
                            }
                        } else {
                            Trap {
                                cause: 2,
                                value: instruction,
                            }
                        })
                    );
                }
            }
        }
    }
}

#[test]
fn hpm_inhibit_bits_leave_cycle_and_instret_controls_independent() {
    for mask in [
        0,
        1,
        4,
        5,
        8,
        0xffff_fff8,
        0xffff_fff9,
        0xffff_fffc,
        0xffff_fffd,
    ] {
        let mut c = cpu();
        c.memory.store(c.pc, 4, 0x13).unwrap(); // nop
        c.csr_write(0x320, mask);
        c.cycles = 11;
        c.retired = 7;
        c.step().unwrap();
        assert_eq!(c.cycles, 11 + u64::from(mask & 1 == 0));
        assert_eq!(c.retired, 7 + u64::from(mask & 4 == 0));
        assert_eq!(c.csr_read(0x320), Some(mask));
        assert_eq!(c.csr_read(0xb03), Some(0));
    }
}
