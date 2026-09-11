#!/usr/bin/env python3
"""Produce the second-round Spike optimization report from saved measurements."""
import hashlib, json, statistics as st, subprocess
from pathlib import Path
from report import md_table

ROOT = Path(__file__).resolve().parents[1]
R2 = ROOT/'results/round2'
OPT = ROOT/'results/optimization'
FINAL = 'opt3'
TAG = 'final3'
KEYS = [('spike','opt'),('spike',FINAL),('qemu','baseline')]
LABELS = {('spike','baseline'):'Spike upstream',('spike','opt'):'Spike optimized (previous round)',('spike',FINAL):'Spike optimized (this round)',('spike','opt2'):'Spike optimized (this round, without console change)',('qemu','baseline'):'QEMU/TCG'}

def read(path):
    return [json.loads(s) for s in Path(path).read_text().splitlines() if s.strip()]
def groups(rows, keys=None):
    keys = keys or sorted({(r['simulator'],r['mode']) for r in rows})
    return {k:[r for r in rows if (r['simulator'],r['mode'])==k and not r['warmup']] for k in keys}
def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def med(rows, key): return st.median(r[key] for r in rows)

boot = read(OPT/f'{TAG}-boot.jsonl'); core = read(OPT/f'{TAG}-coremark.jsonl'); counts = read(OPT/'qemu-counts2.jsonl')
bg, cg = groups(boot, KEYS), groups(core, KEYS)
assert all(len(g)==20 for g in bg.values()), {k:len(g) for k,g in bg.items()}
assert all(len(g)==10 for g in cg.values()), {k:len(g) for k,g in cg.items()}
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
        assert digest(ROOT/Path(r['command'][0]).relative_to(ROOT) if Path(r['command'][0]).is_absolute() else r['command'][0]) == r['binary_sha256']
for line in (ROOT/'results/coremark-existing-binaries-profiles.sha256').read_text().splitlines():
    expected,path=line.split(None,1)
    assert digest(ROOT/path.strip())==expected, f'Original artifact changed: {path}'
prov = json.loads((R2/'provenance.json').read_text())
assert digest(ROOT/'build/spike-opt/spike') == 'aae1a388cfd80a542928f309a8ac7d6a79e784bbb2794ea8e5b58cb58eb5c9f3'
assert digest(ROOT/'build/qemu-baseline/qemu-system-riscv64') == 'ae06fd90fafc6ed85cbba731f4adaa887464e138405d54e9f2b9325579e80efa'

bm={k:med(g,'boot_seconds') for k,g in bg.items()}
cm={k:med(g,'workload_seconds') for k,g in cg.items()}
qcount=st.median(r['instructions'] for r in counts)
mips={k:(st.median(r['instructions']/r['workload_seconds']/1e6 for r in cg[k]) if k[0]=='spike' else qcount/cm[k]/1e6) for k in KEYS}
prev, new, q = KEYS
boot_gain=100*(1-bm[new]/bm[prev]); core_gain=100*(1-cm[new]/cm[prev])
fresh_b=groups(read(OPT/'round2-fresh-boot.jsonl')); fresh_c=groups(read(OPT/'round2-fresh-coremark.jsonl'))
cc=json.loads((R2/'cpu-counters-coremark.json').read_text())
diag=(R2/'diag-counts.txt').read_text().strip().splitlines()

def screen_table(tag, key, labels):
    rows=[]
    for wl,k2 in (('boot','boot_seconds'),('coremark','workload_seconds')):
        g=groups(read(OPT/f'{tag}-{wl}.jsonl'))
        base=med(g[('spike','opt')],k2)
        for var,label in labels.items():
            if ('spike',var) not in g: continue
            m=med(g[('spike',var)],k2)
            rows.append([label, wl, str(len(g[('spike',var)])), f'{m:.6f}', f'{base:.6f}', f'{100*(m/base-1):+.2f}%'])
    return md_table(['Candidate','Workload','N','Candidate median (s)','Previous optimized median (s)','Change'], rows)

def hotspots(name, title, n=10):
    p=json.loads((R2/name).read_text())
    return [f'### {title}', f'{p["samples"]:,} leaf samples; percentages use sample weights, not inclusive stack time.',
            md_table(['Leaf function','Sample weight'],[['`'+r['name'].replace('|','\\|')+'`',f'{r["percent"]:.2f}%'] for r in p['functions'][:n]])]

def sample_table(g, key, fmt):
    labels=[LABELS[k] for k in KEYS]
    idx=sorted({r['index'] for k in KEYS for r in g[k]})
    rows=[]
    for i in idx:
        rows.append([str(i)]+[next((fmt(r[key]) for r in g[k] if r['index']==i),'') for k in KEYS])
    return md_table(['Index']+labels, rows)

t=['# Spike optimization on Apple M5, second round: no PGO',
f'**The QEMU/TCG target was not reached.** This round adds four architecture-preserving changes on Spike branch `codex-opt`: instruction caches kept per privilege regime, exact invalidation for address-specific `sfence.vma` with a larger decoded-instruction cache, frameless load/store fast paths, and rate-limited host console polling. Against the previous optimized Spike, median Linux boot time falls by **{boot_gain:.1f}%** ({bm[prev]*1000:.2f} ms to **{bm[new]*1000:.2f} ms**) and the median CoreMark workload time falls by **{core_gain:.1f}%** ({cm[prev]:.3f} s to **{cm[new]:.3f} s**, {mips[new]:.0f} MIPS). QEMU boots in **{bm[q]*1000:.2f} ms** and runs CoreMark in **{cm[q]:.3f} s** ({mips[q]:.0f} MIPS), so QEMU remains **{bm[new]/bm[q]:.2f}× faster at boot** and **{cm[new]/cm[q]:.2f}× faster on CoreMark**.',
'Hardware counters explain why the remaining gap is not closable with clean interpreter changes: inside the CoreMark loop Spike already retires host instructions at an IPC of about 6, so it is not stalled; it simply executes about three times as many host instructions per guest instruction as QEMU\'s translated code. Details, the rejected experiments, and the remaining hotspots follow.',
'## Final measurements',
'All Spike builds use Homebrew LLVM 20.1.8 with `-O3 -flto=thin` and **no PGO**. "Previous round" is the unchanged `build/spike-opt/spike` executable from the first optimization report (data-permission cache retention only); QEMU is the unchanged baseline executable. Both were hash-checked before and after this series.',
md_table(['Build','Boot median (ms)','CoreMark median (s)','CoreMark MIPS'],[[LABELS[k],f'{bm[k]*1000:.3f}',f'{cm[k]:.6f}',f'{mips[k]:.2f}'] for k in KEYS]),
(lambda g: 'The boot series above contains a few slow outliers in every build (retained per protocol); a supplementary boot-only series of another 20 runs per build taken immediately afterwards ([JSONL](results/optimization/final3b-boot.jsonl)) gives medians of ' + ', '.join(f'{LABELS[k]} {med(g[k],"boot_seconds")*1000:.2f} ms' for k in KEYS) + ', the same picture.')(groups(read(OPT/'final3b-boot.jsonl'),KEYS)),
'The final series contains 20 measured boot runs and 10 measured CoreMark runs per build, each preceded by two warmups, with the order alternating every round. No profiler, compiler, or counting plugin ran concurrently. All outliers are retained. Timing uses the host clock (`perf_counter_ns()`) between console markers; guest timers are ignored.',
f'Spike retires exactly **9,719,260,233** guest instructions in every CoreMark run, before and after the changes, so the work is identical. QEMU\'s MIPS uses **{qcount:,.0f}** median instructions from three separate plugin-instrumented runs divided by its uninstrumented median; it is an estimate with slightly different counting semantics.',
'A fresh comparison taken at the start of this round, before any new build, reproduced the previous report: ' + ', '.join(f'{LABELS.get(k,k[0]+":"+k[1])} boot {med(g,"boot_seconds")*1000:.1f} ms' for k,g in fresh_b.items()) + '; ' + ', '.join(f'{LABELS.get(k,k[0]+":"+k[1])} CoreMark {med(g,"workload_seconds"):.3f} s' for k,g in fresh_c.items()) + ' ([boot](results/optimization/round2-fresh-boot.jsonl), [CoreMark](results/optimization/round2-fresh-coremark.jsonl)).',
'## Where the time goes',
'### CoreMark: instruction count, not stalls',
'The Instruments *CPU Counters* template was attached for three seconds inside the CoreMark loop of each simulator. Its default configuration records raw per-core PMU arrays without event names, so the columns were identified with a calibration program of known instruction, branch, and cache-miss counts ([calib.c](results/round2/calib.c), [calib2.c](results/round2/calib2.c), [pmc.py](results/round2/pmc.py)): column 0 is cycles, column 1 instructions retired, column 2 micro-ops retired, column 4 micro-ops scheduled, column 5 front-end delivery bubbles. Guest instruction counts in the window are estimated from the fresh-series median MIPS, so the per-guest-instruction figures are approximate to a few percent. [Raw sums](results/round2/cpu-counters-coremark.json).',
md_table(['Simulator','Host GHz','Host IPC','Host instructions per guest instruction','Host cycles per guest instruction','Micro-ops discarded'],
  [[n, f'{d["ghz"]:.2f}', f'{d["ipc"]:.2f}', f'{d["host_instructions_per_guest_instruction"]:.1f}', f'{d["cycles_per_guest_instruction"]:.2f}', f'{100*d["uops_discarded_fraction"]:.1f}%'] for n,d in (('Spike (previous round)',cc['spike_opt']),('QEMU/TCG',cc['qemu_baseline']))]),
'Both programs run near the M5 P-core\'s sustainable width. Spike\'s call-threaded dispatch loop is 13 host instructions per guest instruction (load handler and instruction, load next entry, two argument moves, indirect call, next-tag load and compare, PC store, counter update, budget compare) and the generated handlers add roughly 8 to 40 more each: register-index and immediate extraction from raw instruction bits, register-file access, TLB tag computation and compare, and, for loads and stores, a stack frame kept for the slow path. Matching QEMU would need about 9 host instructions per guest instruction, less than the dispatch loop alone, which is why no handler-level or cache-level change can close a threefold gap. Only amortizing decode and dispatch across blocks (translation) changes that arithmetic, and that was excluded from this task.',
'### Linux boot: cache invalidation',
'Counters added to a diagnostic build ([counts](results/round2/diag-counts.txt), build not retained) show that the previous optimized Spike performed 8,607 privilege switches and 11,361 full cache flushes while booting, causing 7.66 million decoded-instruction refills for 76 million executed instructions. On the CoreMark image the same mechanisms produced 32,157 flushes and 18.5 million refills. After per-regime caches, the remaining 2,754 boot flushes were almost all address-specific `sfence.vma` (2,723), which still discarded everything, and the 4,096-entry direct-mapped decoded-instruction cache still missed 6.2 million times because the boot touches 120,004 distinct instruction addresses ([PC histogram](results/instruction-counts-spike-baseline-count-01.log)).',
'## Retained changes',
'All four are in the Spike submodule on branch `codex-opt`, in [mmu.h](spike/riscv/mmu.h), [mmu.cc](spike/riscv/mmu.cc), [processor.cc](spike/riscv/processor.cc), [sfence_vma.h](spike/riscv/insns/sfence_vma.h), and [term.cc](spike/fesvr/term.cc). The instruction handler ABI, the dispatch loop, trap delivery, retirement accounting, and all guest-visible behavior are unchanged. Combined patch: [round2.patch](results/round2/round2.patch).',
'### 1. Instruction caches per privilege regime',
'The decoded-instruction cache and the instruction TLB are only valid for the (privilege, virtualization) regime that filled them, because fetch permission depends on the current privilege and on U-bit pages. Instead of discarding both on every privilege change, the MMU now keeps one set per regime (allocated on first use; U, S, and M on this workload) and a privilege change selects a set. Two epoch counters implement lazy invalidation: `flush_tlb()` and `flush_icache()` (satp/hgatp/PMP/misa writes, trigger updates, `sfence.vma` without an address, `fence.i`, memory-tracer registration) increment an epoch, clear the current set immediately, and clear any other set the first time it is selected again. The data TLBs and the page-table-entry cache keep their existing behavior and are still flushed on every privilege change, so effective-privilege subtleties of `MPRV`, `MPP`, `mnstatus.NMIE`, `SUM`, and `MXR` are handled exactly as before. Debug Mode changes which memory is accessible, so entering or leaving it still discards every set. `Ziccid` store-side invalidation scans every current set. Machine mode never translates fetches, so it is one regime like the others.',
'### 2. Exact invalidation for `sfence.vma` with an address',
'`sfence.vma rs1, rs2` with `rs1 != x0` orders only the leaf page table entry for that virtual address. Every TLB entry now records the log2 size of the leaf page it was derived from (recorded by the page walk; two-stage and untranslated accesses record an unknown size that matches every fence), so the fence invalidates exactly the entries of that leaf page in the load, store, and per-regime instruction TLBs, including sibling 4 KiB granules of a superpage. Cached page-table entries are dropped entirely. Each instruction-cache set keeps a Bloom filter of the leaf pages it fetched from and a mask of the page sizes seen; a fence skips sets that never fetched from the page and scans only the aligned index block a smaller-than-cache page can occupy. ASIDs are ignored as before (all address spaces are invalidated), `sfence.vma x0` keeps the full invalidation, and `hfence.*` are unchanged. With address fences no longer wiping the caches, the decoded-instruction cache was enlarged from 4,096 to 65,536 entries (2 MiB per regime, allocated on first use); 16,384 was measured too, see below.',
'### 3. Frameless load and store fast paths',
'Every load and store handler kept a full stack frame, four callee-saved registers, and a zero-initialized stack temporary only because the TLB-miss path calls `load_slow_path`/`store_slow_path` with a pointer to that temporary. The slow paths are now outlined into `load_slow<T>`/`store_slow<T>` helpers declared `noinline` and, where the compiler supports it (Clang on AArch64 and x86-64, not Windows), `preserve_most`, so the callee preserves the caller\'s registers. With the fast path no longer needing a frame, `lh` shrinks from 36 to 26 host instructions and `c.ld` from 40 to 30; the link register is saved only in the cold block. Trap semantics are unchanged: the helpers throw the same exceptions through the same frames, and Linux boot (which takes thousands of page faults through this path) and the permission fixture exercise them.',
'### 4. Host console polling',
'The HTIF console device asks the host for keyboard input on every HTIF tick, i.e. after every 5,000 guest instructions, and each check is a `poll()` system call (two system calls when standard input is `/dev/null`, as in this harness, because it always reports readable). The Time Profiler attributed about 7% of CoreMark samples to `poll`. `canonical_terminal_t::read()` in [fesvr/term.cc](spike/fesvr/term.cc) now checks the host at most once per millisecond of host time between bursts (a burst is still drained on consecutive calls), and stops checking once standard input has reached end of file. Guest-visible behavior is unchanged apart from at most one millisecond of added latency for typed input; the guest\'s pending read is answered from the same queue as before.',
'## Experiments',
'Screening series interleave the candidate with the previous optimized build (10 boot runs and 3 CoreMark runs each, one warmup). Changes of about 1% between different builds are within the run-to-run and code-layout noise seen here and are not treated as established.',
'### Candidates screened',
screen_table('screen-abd','', {'regime':'A: per-regime caches','cold':'B: frameless load/store','regime2':'A + coarse address fence (rejected)'}),
screen_table('screen-exact','', {'regime':'A: per-regime caches','exact16k':'A + exact address fence, 16K entries','exact64k':'A + exact address fence, 64K entries'}),
'The console change was screened on top of the three MMU/handler changes (`opt2`), which had already been confirmed with 20 boot and 10 CoreMark runs ([boot](results/optimization/final2-boot.jsonl), [CoreMark](results/optimization/final2-coremark.jsonl): ' + ', '.join(f'{LABELS[k]} boot {med(g,"boot_seconds")*1000:.1f} ms' for k,g in groups(read(OPT/'final2-boot.jsonl'),[('spike','opt'),('spike','opt2'),('qemu','baseline')]).items()) + '; ' + ', '.join(f'{LABELS[k]} CoreMark {med(g,"workload_seconds"):.3f} s' for k,g in groups(read(OPT/'final2-coremark.jsonl'),[('spike','opt'),('spike','opt2'),('qemu','baseline')]).items()) + ').',
(lambda g: md_table(['Candidate','Workload','N','Candidate median (s)','opt2 median (s)','Change'], [[ 'opt2 + console polling', wl, str(len(g[wl][('spike','opt3')])), f'{med(g[wl][("spike","opt3")],k):.6f}', f'{med(g[wl][("spike","opt2")],k):.6f}', f'{100*(med(g[wl][("spike","opt3")],k)/med(g[wl][("spike","opt2")],k)-1):+.2f}%'] for wl,k in (('boot','boot_seconds'),('coremark','workload_seconds'))]))({wl:groups(read(OPT/f'screen-console-{wl}.jsonl')) for wl in ('boot','coremark')}),
'A first version of the address fence did not record page sizes and instead discarded every translation in the largest possible leaf page (1 GiB under Sv39) by scanning all sets: it removed the flushes but gained nothing, because most refills were capacity misses of the 4,096-entry cache and the scans cost as much as the flushes ([patch](results/round2/fetch-regime-sfence.patch), [screening](results/optimization/screen-abd-coremark.jsonl)). Recording leaf sizes and filtering the scan made a larger cache affordable. The 65,536-entry variant ([patch](results/round2/fetch-regime-exact-sfence-64k.patch)) booted about 4% faster than 16,384 entries ([patch](results/round2/fetch-regime-exact-sfence-16k.patch)) at the cost of 2 MiB per regime per hart, and was retained; the constant is `fetch_cache_t::ICACHE_ENTRIES`.',
'### Rejected: publishing the PC only on loop exit',
'Removing the per-instruction `state.pc` store from the fast loop is possible in principle (the catch blocks and the serialization sentinels would publish it), but it saves one store out of about 28 host instructions per guest instruction and touches the precise-trap path for at most a 1-2% effect that this setup cannot distinguish from layout noise; it was not built. The unroll, load-helper-without-`preserve_most`, and privilege-number-only flush experiments of the previous round were not repeated.',
'## Profiles after the changes'] + hotspots(f'{FINAL}-coremark-hotspots.json','CoreMark steady state (this round)') + hotspots(f'{FINAL}-boot-hotspots.json','Linux boot (this round, five launches aggregated)') + [
'Remaining boot hotspots: `memset` and `fence.i` together are about 11% of samples, because each of the 213 `fence.i` instructions and each remaining global flush during boot clears a 2 MiB decoded-instruction set (the previous build cleared 128 KiB but did so 11,361 times); refills fell from 13% to 3% of samples. Traps are delivered as C++ exceptions; the unwinder (`libunwind`, `dyld` section lookups) is visible at a few percent of boot samples and is inherent to the current trap architecture. Compressed exported samples: [CoreMark](results/round2/opt3-coremark.xml.gz), [boot](results/round2/opt3-boot.xml.gz).',
'## Validation',
'- [tests/spike-fetch-regime.cc](tests/spike-fetch-regime.cc) (new) links the static Spike libraries and checks: user-mode entries are invisible to supervisor mode and vice versa (SUM does not extend to fetch); a privilege round trip may retain a decoded instruction; `fence.i` and `sfence.vma`/`satp` writes issued in supervisor mode reach the user regime; execute revocation is observed after `sfence.vma`; machine mode uses physical addresses; Debug Mode entry and exit discard everything; with `-DTEST_ADDRESS_FENCE`, an address fence discards the fenced 4 KiB page in the current and the user regime while keeping a page one gigabyte away, a fence anywhere inside a 2 MiB leaf discards both instruction and data translations of every 4 KiB granule of that leaf while keeping a neighboring 4 KiB page, and a 4 KiB fence afterwards still works; an instruction straddling the end of a 2 MiB leaf and a 4 KiB page, fetched with commit logging enabled (which disables TLB fills), is still discarded by a fence for another granule of the superpage; with the hypervisor extension, VS mode is a separate regime from HS mode even with a bare G-stage and `fence.i` crosses them. The straddle check was added after a review of the patch found that the page size could be taken from the wrong page in that configuration; the [pre-fix binary fails exactly there](results/round2/fixture-fetch-regime-opt2-prefix.log) and the [retained build passes](results/round2/fixture-fetch-regime-opt3.log).',
'- [tests/spike-status-tlb.cc](tests/spike-status-tlb.cc) (previous round) still passes: SUM/MXR/MPRV/MPP permission changes with populated caches.',
'- [tests/spike-console-poll.cc](tests/spike-console-poll.cc) (new) links `libfesvr.a` and checks that a burst of input is drained on consecutive calls, that input arriving between polls is delivered within the documented interval while intermediate calls do not poll, and that nothing is delivered after end of input. It [fails on the previous build](results/round2/fixture-console-poll-opt2.log) because that build polls on every call.',
'- Every final guest run reaches BusyBox and powers down; every CoreMark run matches the expected CRCs, iteration count, exit status, and the identical `instret` delta. The new fetch-regime fixture, linked against the previous build with its own headers, fails only at the first retention check, which is the intended behavioral difference.',
'Fixture logs: [fetch-regime](results/round2/fixture-fetch-regime-opt3.log), [status-tlb](results/round2/fixture-status-tlb-opt3.log), [console-poll](results/round2/fixture-console-poll-opt3.log). The fixtures are targeted regression coverage, not architectural certification; in particular NAPOT pages, two-stage superpage combinations, and memory tracers are covered only by the conservative "unknown size" and full-flush paths.',
'## Limitations',
f'- QEMU/TCG remains {bm[new]/bm[q]:.2f}× faster at boot and {cm[new]/cm[q]:.2f}× faster on CoreMark. The counter data above bounds what interpreter-level changes can do: dispatch alone costs more host instructions than QEMU spends per guest instruction.',
'- Per-hart memory grows by about 2 MiB per regime used (three regimes on Linux; five with the hypervisor extension). Address fences cost a few microseconds of scanning each; global fences and `fence.i` now clear 2 MiB per affected regime.',
'- `preserve_most` is a Clang extension; other compilers fall back to plain `noinline` helpers and keep a frame.',
'- Measurements are subject to macOS scheduling, frequency, and thermal variation; no CPU pinning was used. Code layout differences between builds can move CoreMark by about 1%.',
'## Reproduce',
'```sh\n' + '\n'.join([
    'git -C spike checkout codex-opt  # or apply results/round2/round2.patch to 388e10a',
    'bash scripts/build-spike-variant.sh opt3',
    'FIXTURE_FLAGS=-DTEST_ADDRESS_FENCE bash scripts/test-spike-fixture.sh tests/spike-fetch-regime.cc opt3',
    'bash scripts/test-spike-fixture.sh tests/spike-status-tlb.cc opt3',
    'bash scripts/test-spike-console-poll.sh opt3',
    'bash scripts/profile-round2.sh opt3',
    'python3 scripts/benchmark-optimization.py --tag final3 --workload boot --runs 20 --warmups 2 --variants spike:opt spike:opt3 qemu:baseline',
    'python3 scripts/benchmark-optimization.py --tag final3 --workload coremark --runs 10 --warmups 2 --variants spike:opt spike:opt3 qemu:baseline',
    'python3 scripts/benchmark-coremark.py --iterations 32000 --simulators qemu --modes baseline --counted --runs 3 --warmups 0 --output results/optimization/qemu-counts2.jsonl',
    'python3 scripts/provenance-round2.py',
    'python3 scripts/report-round2.py'
]) + '\n```',
'Run builds, profiling, and timing serially with normal macOS tool access. The timing runners refuse to overwrite their JSONL outputs. Counter recordings: `xcrun xctrace record --template "CPU Counters" --time-limit 3s --attach <pid>` while the CoreMark loop runs, then `python3 results/round2/pmc.py <trace> 0 [thread-substring]`; keep recordings short, the template writes several hundred megabytes per second.',
'## Every final timing sample',
f'Negative indices are excluded warmups. Times are seconds. [Boot JSONL](results/optimization/{TAG}-boot.jsonl), [CoreMark JSONL](results/optimization/{TAG}-coremark.jsonl).',
'### Boot', sample_table(groups(boot,KEYS)|{k:[r for r in boot if (r['simulator'],r['mode'])==k] for k in KEYS},'boot_seconds',lambda v:f'{v:.6f}'),
'### CoreMark', sample_table({k:[r for r in core if (r['simulator'],r['mode'])==k] for k in KEYS},'workload_seconds',lambda v:f'{v:.6f}'),
'### Separate QEMU instruction-counting runs',
md_table(['Index','Instructions','Instrumented workload (s)'],[[str(r['index']),f'{r["instructions"]}',f'{r["workload_seconds"]:.6f}'] for r in counts]),
'## Provenance',
'[Build and host metadata](results/round2/provenance.json), retained patches ([per-regime caches](results/round2/fetch-regime.patch), [exact address fence and 64K cache](results/round2/fetch-regime-exact-sfence-64k.patch), [frameless load/store](results/round2/cold-slow-path.patch), [console polling](results/round2/console-poll.patch), [combined](results/round2/round2.patch)), and [unchanged original artifacts](results/round2/unchanged-artifacts-check.txt).',
md_table(['Executable','SHA-256'],[[p,digest(ROOT/p)] for p in ('build/spike-baseline/spike','build/spike-opt/spike','build/spike-opt2/spike','build/spike-opt3/spike','build/qemu-baseline/qemu-system-riscv64')]),
f'Spike submodule: branch `codex-opt`, commit `{prov["spike_commit"]}` on top of the previous round\'s `388e10aa247a52ce1b072ce7e477cdf5b521158f`; the same change is saved as [round2.patch](results/round2/round2.patch). Candidate executables that were measured but not retained are kept with their hashes in [candidate-binaries.sha256](results/round2/candidate-binaries.sha256); their build trees were deleted to free disk space.',
]
(ROOT/'OPTIMIZATION_REPORT_ROUND2.md').write_text('\n\n'.join(t)+'\n')
print('wrote OPTIMIZATION_REPORT_ROUND2.md')
