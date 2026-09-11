#!/usr/bin/env python3
"""Produce the non-PGO optimization report from saved measurements."""
import hashlib
import json
from pathlib import Path
import statistics as st
from report import md_table

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'results/optimization'
KEYS = [('spike','baseline'),('spike','opt'),('qemu','baseline')]
LABELS = ['Spike upstream','Spike optimized','QEMU/TCG']
def read(name):
    return [json.loads(s) for s in (OUT/name).read_text().splitlines() if s]
def groups(rows):
    return {k:[r for r in rows if (r['simulator'],r['mode'])==k and not r['warmup']] for k in KEYS}
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

boot = read('final-boot.jsonl')
core = read('final-coremark.jsonl')
counts = read('qemu-counts.jsonl')
bg,cg = groups(boot),groups(core)
assert all(len(g)==20 for g in bg.values())
assert all(len(g)==10 for g in cg.values())
assert len(counts)==3 and all(r['simulator']=='qemu' and r['mode']=='baseline' and r['counted'] for r in counts)
assert len({tuple(sorted(r['crcs'].items())) for r in core+counts})==1
assert all(r['iterations']==32000 for r in core+counts)
assert {r['instructions'] for r in core if r['simulator']=='spike'} == {9719260233}
for r in boot+core+counts:
    assert r['mode'] not in ('pgo','train')
    data=(ROOT/r['log']).read_bytes()
    assert b'BENCH_BUSYBOX_READY' in data and b'Power down' in data
    if 'crcs' in r:
        assert data.count(b'BENCH_COREMARK_START') == data.count(b'BENCH_COREMARK_END') == 1
        assert b'BENCH_COREMARK_EXIT=0' in data
    if 'binary_sha256' in r:
        assert digest(Path(r['command'][0])) == r['binary_sha256']
for line in (ROOT/'results/coremark-existing-binaries-profiles.sha256').read_text().splitlines():
    expected,path=line.split(None,1)
    assert digest(ROOT/path.strip())==expected, f'Original artifact changed: {path}'
bm={k:st.median(r['boot_seconds'] for r in g) for k,g in bg.items()}
cm={k:st.median(r['workload_seconds'] for r in g) for k,g in cg.items()}
qcount=st.median(r['instructions'] for r in counts)
mips={k:st.median(r['instructions']/r['workload_seconds']/1e6 for r in cg[k])
      if k[0]=='spike' else qcount/cm[k]/1e6 for k in KEYS}
boot_gain=100*(1-bm[KEYS[1]]/bm[KEYS[0]])
core_change=100*(cm[KEYS[1]]/cm[KEYS[0]]-1)
t=['# Spike optimization on Apple M5: no PGO',
   f'**QEMU/TCG remains fastest.** The retained Spike patch reduces median Linux boot time by **{boot_gain:.1f}%**, from **{bm[KEYS[0]]*1000:.2f} ms** to **{bm[KEYS[1]]*1000:.2f} ms**. QEMU boots in **{bm[KEYS[2]]*1000:.2f} ms**. CoreMark takes **{cm[KEYS[1]]:.3f} s** on optimized Spike versus **{cm[KEYS[2]]:.3f} s** on QEMU.',
   'The requested performance target was **not reached**. One small, architecture-preserving cache-invalidation improvement is retained on Spike branch `codex-opt`. No clean change found in this investigation closes the remaining interpreter-versus-TCG gap; this is an experimental conclusion, not a proof that every possible interpreter optimization has been exhausted.',
   '## Final measurements',
   'All three binaries use Homebrew LLVM 20.1.8, `-O3 -flto=thin`, with **no PGO generation or use**. QEMU and upstream Spike are the unchanged original baseline executables. Existing PGO binaries and profile files were hash-checked but never executed or consumed in this experiment.',
   md_table(['Build','Boot median (ms)','CoreMark median (s)','CoreMark MIPS'],
            [[label,f'{bm[k]*1000:.3f}',f'{cm[k]:.6f}',f'{mips[k]:.2f}'] for k,label in zip(KEYS,LABELS)]),
   f'QEMU is **{bm[KEYS[1]]/bm[KEYS[2]]:.2f}× faster at boot** and **{cm[KEYS[1]]/cm[KEYS[2]]:.2f}× faster on CoreMark** than optimized Spike. The optimized Spike CoreMark median changes by {core_change:+.2f}% relative to upstream; small differences at this level should not be treated as established throughput improvements.',
   'The final series contains 20 measured boot runs and 10 measured CoreMark runs per build, each preceded by two warmups. Order alternates every round. No profiler, compiler, or instruction-counting plugin ran concurrently with these timed series. All outliers are retained.',
   'CoreMark executes the same 32,000 iterations, seeds, binary, and single context on all simulators. Its workload interval uses host `perf_counter_ns()` between console markers after BusyBox starts. Boot uses the separate original boot-only firmware and times process launch to `BENCH_BUSYBOX_READY`. Both platforms have one hart, 256 MiB RAM, RVA22-compatible ISA, the same HTIF/CLINT/Sv39 topology, and no hardware virtualization. See [the original recipe](README.md) and [CoreMark methodology](COREMARK_REPORT.md).',
   f'Spike retires exactly **9,719,260,233** instructions in every CoreMark run, before and after the patch. Its MIPS uses each run’s `instret` delta divided by that run’s host time. QEMU uses **{qcount:,.0f}** median instructions from three separate plugin runs divided by its uninstrumented median runtime; this is an estimate with slightly different counting semantics. QEMU’s guest `instret` and both guest-clock CoreMark scores are ignored. This short workload comparison is not an official CoreMark score.',
   '## Profile and source analysis',
   'The baseline CoreMark Time Profiler recording was time-limited to 25 seconds. The analysis discards the first two seconds of trace time to focus on steady execution; it is a sampled interval, not a claim that the profiled guest completed. The separate baseline boot trace includes startup through exit. Profiling changes runtime, so none of these trace durations is a benchmark timing. The initial boot trace overlapped a candidate build; its percentages are diagnostic, not a precise serial-time decomposition.',
   'Compressed exported samples and machine-readable summaries are retained in [results/optimization](results/optimization). Raw Instruments traces and disassembly stay in ignored `build/optimization/`.',
]
for title,file in [('CoreMark steady state','baseline-hotspots.json'),('Linux boot','boot-hotspots.json'),('Optimized Linux boot','boot-opt-hotspots.json')]:
    p=json.loads((OUT/file).read_text())
    t += [f'### {title}',f'{p["samples"]:,} leaf samples; percentages use sample weights, not inclusive stack time.',
          md_table(['Leaf function','Sample weight'],[['`'+r['name'].replace('|','\\|')+'`',f'{r["percent"]:.2f}%'] for r in p['functions'][:10]])]
t += ['### Retained change: flush data translations when data permissions change',
      'The original `base_status_csr_t::maybe_flush_tlb()` invalidated instruction and data translations plus all 4,096 decoded instruction-cache entries whenever MPP, MPRV, SUM, or MXR changed. The baseline boot profile attributes 21.7% of leaf samples to `mstatus_csr_t::unlogged_write()`; disassembly places most of those samples in its inlined 4,096-entry invalidation loop.',
      'The patch adds `mmu_t::flush_data_tlb()` and uses it for those status changes. It invalidates both load/store TLBs and conservatively clears the page-table-entry cache. Full invalidation still clears instruction translations and decoded instructions for existing callers, including privilege changes and translation fences. `fence.i` retains its original invalidation path.',
      'MPP/MPRV select the effective privilege for data accesses; SUM controls supervisor data access to user pages, and MXR allows loads from executable pages. Instruction fetch is independent of those settings. This agrees with the [RISC-V privileged specification](https://docs.riscv.org/reference/isa/priv/machine.html) and Spike’s `generate_access_info()`, `walk()`, and `s2xlate()` implementations. In two-stage translation, Spike explicitly excludes MXR from implicit VS page-table reads.',
      'Patch: [data-tlb.patch](results/optimization/data-tlb.patch). Source: [csrs.cc](spike/riscv/csrs.cc), [mmu.cc](spike/riscv/mmu.cc), [mmu.h](spike/riscv/mmu.h). Instruction implementations, dispatch ABI, cache lookup rules, trap handling, guest binaries, and ISA capabilities are unchanged.',
      '### CoreMark limit',
      'The baseline steady-state profile places 35.4% of samples in `processor_t::step()` and 59.5% in instruction handlers. The generated fast loop loads the cached handler and instruction, performs an indirect call, validates the next cache entry, updates the architectural PC, and checks the retirement budget for every guest instruction. Handlers repeatedly extract register indices and immediates, access the guest register file, and check memory translations or branch alignment.',
      'Instruction decoding on cache misses is not a dominant CoreMark hotspot. Increasing decode-cache capacity therefore does not address the observed bottleneck. Even removing the sampled dispatch cost entirely gives an illustrative Amdahl bound of only about 1.55×, assuming all remaining costs stay fixed—well below the required roughly 3.5×. Sampling cannot predict all microarchitectural interactions, so this is explanatory, not a hard hardware limit.',
      'QEMU amortizes decoding and dispatch across translated blocks and can chain blocks directly; see its [translator internals](qemu/docs/devel/tcg.rst). Closing the remaining gap would likely require richer predecoded operations, instruction fusion, threaded execution, or native code generation. These require broader changes to execution boundaries, precise traps, tracing, single stepping, and invalidation. No benchmark-specific shortcuts or new execution engine were introduced.',
      '## Other ideas tested or rejected',
      md_table(['Candidate','Upstream CoreMark median (s)','Candidate median (s)','Decision'],
        [[name,
          f'{st.median(r["workload_seconds"] for r in groups(read(tag+"-coremark.jsonl"))[KEYS[0]]):.6f}',
          f'{st.median(r["workload_seconds"] for r in groups(read(tag+"-coremark.jsonl"))[KEYS[1]]):.6f}',decision]
         for name,tag,decision in [('Unroll dispatch four times','unroll4','Slower; reverted'),
                                   ('Outline load temporary into slow helper','load-slow','Only 0.9% lower median; not retained'),
                                   ('Data-only status invalidation','data-tlb-screen','Retained for clear boot improvement')]]),
      'These screening series use three measured runs and one warmup per build. Patches and executable hashes are saved for both reverted experiments. The unroll increased dispatch code size and was slower despite creating multiple indirect-call sites. The load helper removed a stack temporary but retained register-save overhead; its small apparent gain did not justify keeping it without stronger evidence.',
      'Every screening sample, including warmups and outliers: [unroll](results/optimization/unroll4-coremark.jsonl), [load helper](results/optimization/load-slow-coremark.jsonl), [data-only flush CoreMark](results/optimization/data-tlb-screen-coremark.jsonl), and [data-only flush boot](results/optimization/data-tlb-screen-boot.jsonl). Reverted source experiments: [unroll patch](results/optimization/unroll4.patch), [load-helper patch](results/optimization/load-slow.patch).',
      'Skipping a full flush solely because `set_privilege()` receives the current privilege was rejected during source review: debug transitions and `mnret` can change effective memory access through `debug_mode` or `mnstatus.NMIE` without changing that privilege. The existing flush is required across those transitions. Likewise, preloading the next decoded instruction across an arbitrary handler risks using entries invalidated by `fence.i` or other state changes.',
      '## Validation and limits',
      'The [native regression fixture](tests/spike-status-tlb.cc) runs against both original and patched static Spike libraries. With populated caches, it checks SUM load/store permission revocation, supervisor fetch rejection on user pages, MXR enable/disable on execute-only pages, MPRV and MPP translation changes, and visibility of instruction modification/remapping after explicit invalidation. It covers RV64 supervisor mode with H absent/present, virtual supervisor mode with bare G-stage, and HS MXR effects on VS loads. Both builds pass. This is targeted regression coverage, not full RISC-V architectural certification.',
      'Every final guest run reaches BusyBox and powers down successfully; every CoreMark run matches all expected CRCs, iterations, and exit status. The optimized and original Spike instruction deltas are identical. Validation logs: [upstream](results/optimization/status-test-baseline.log), [optimized](results/optimization/status-test-opt.log).',
      'Wall times are subject to macOS scheduling, core migration, frequency changes, and console notification latency. No CPU affinity was forced. The long workload is dominated by computation, while boot includes loader/startup overhead. Spike’s instruction-driven timer and QEMU’s virtual clock retain the differences documented in the original reports. Results apply to these binaries and this workload on this M5 Mac.',
      '## Reproduce',
      'Keep the original non-PGO baseline binaries and existing guest images. With the retained patch present in the Spike worktree:',
      '```sh\nbash scripts/build-spike-opt.sh\nbash scripts/test-spike-status-tlb.sh baseline\nbash scripts/test-spike-status-tlb.sh opt\npython3 scripts/profile-spike.py --mode opt --workload boot --name boot-opt\npython3 scripts/benchmark-optimization.py --tag final --workload boot --runs 20 --warmups 2\npython3 scripts/benchmark-optimization.py --tag final --workload coremark --runs 10 --warmups 2\npython3 scripts/benchmark-coremark.py --iterations 32000 --simulators qemu --modes baseline \\\n  --counted --runs 3 --warmups 0 --output results/optimization/qemu-counts.jsonl\npython3 scripts/report-optimization.py\n```',
      'Run profiling and benchmarking with normal macOS tool access, serially, after builds finish. Archive old result files before reusing names; the timing runners refuse to overwrite their JSONL outputs. For baseline profiles, use `profile-spike.py --mode baseline --workload boot|coremark` with a new name. `summarize-spike-profile.py` also reads the saved compressed XML directly.',
      '## Every final timing sample',
      'Negative indices are excluded warmups. Times are seconds. Raw logs and full commands are linked from the [boot JSONL](results/optimization/final-boot.jsonl) and [CoreMark JSONL](results/optimization/final-coremark.jsonl).']
for title,rows,field in [('Boot',boot,'boot_seconds'),('CoreMark',core,'workload_seconds')]:
    t += ['### '+title, md_table(['Index',*LABELS],
         [[str(i),*[f'{next(r[field] for r in rows if r["index"]==i and (r["simulator"],r["mode"])==k):.6f}' for k in KEYS]]
          for i in sorted({r['index'] for r in rows})])]
t += ['### Separate QEMU instruction-counting runs',
      md_table(['Index','Instructions','Instrumented workload (s)'],
               [[str(r['index']),str(r['instructions']),f'{r["workload_seconds"]:.6f}'] for r in counts]),
      'Instrumented times are shown only to expose counting overhead; they are not used as benchmark timings. [Count records](results/optimization/qemu-counts.jsonl).',
      '## Provenance',
      '[Build and host metadata](results/optimization/provenance.json), [retained patch](results/optimization/data-tlb.patch), and [unchanged original artifacts](results/optimization/unchanged-artifacts-check.txt). The Spike submodule remains on `codex-opt`; this report does not create a commit.',
      md_table(['Executable','SHA-256'],[[str(Path(bg[k][0]['command'][0]).relative_to(ROOT)),bg[k][0]['binary_sha256']] for k in KEYS])]
(ROOT/'OPTIMIZATION_REPORT.md').write_text('\n\n'.join(t)+'\n')
