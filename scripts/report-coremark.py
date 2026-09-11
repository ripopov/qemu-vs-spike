#!/usr/bin/env python3
"""Report CoreMark runs without rebuilding simulators or collecting PGO."""
import hashlib
import json
from pathlib import Path
import statistics as st
from report import md_table
ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT/'results/coremark'
KEYS = [('spike','baseline'),('qemu','baseline'),('spike','pgo'),('qemu','pgo')]
def read(name):
    return [json.loads(s) for s in (RESULTS/name).read_text().splitlines() if s]
def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    timing = read('measurements.jsonl')
    counting = read('instruction-counts.jsonl')
    groups = {k:[r for r in timing if (r['simulator'],r['mode'])==k and not r['warmup']] for k in KEYS}
    counts = {m:[r for r in counting if r['mode']==m and not r['warmup']] for m in ['baseline','pgo']}
    assert all(len(g)==10 for g in groups.values())
    assert all(len(g)==3 for g in counts.values())
    assert len({r['iterations'] for r in timing+counting})==1
    assert len({tuple(sorted(r['crcs'].items())) for r in timing+counting})==1
    for r in timing+counting:
        data=(ROOT/r['log']).read_bytes()
        assert data.count(b'BENCH_COREMARK_START')==data.count(b'BENCH_COREMARK_END')==1
        assert b'BENCH_COREMARK_EXIT=0' in data and b'Power down' in data
    for line in (ROOT/'results/coremark-existing-binaries-profiles.sha256').read_text().splitlines():
        expected,path=line.split(None,1)
        assert digest(ROOT/path.strip())==expected, f'Existing artifact changed: {path}'
    iterations=timing[0]['iterations']
    med={k:st.median(r['workload_seconds'] for r in g) for k,g in groups.items()}
    mips={}
    for (s,m),g in groups.items():
        if s=='spike':
            mips[s,m]=st.median(r['instructions']/r['workload_seconds']/1e6 for r in g)
        else:
            mips[s,m]=st.median(r['instructions'] for r in counts[m])/med[s,m]/1e6
    rev=(RESULTS/'source-revision.txt').read_text().strip()
    qratio=med['spike','pgo']/med['qemu','pgo']
    t=['# CoreMark on Linux: Spike versus QEMU on Apple M5',
       f'**QEMU is {qratio:.2f}× faster than Spike with the existing boot-trained PGO builds.** For {iterations:,} fixed iterations, median CoreMark workload time is **{med["qemu","pgo"]:.3f} s** on QEMU and **{med["spike","pgo"]:.3f} s** on Spike. All measured runs passed the expected algorithm CRC checks.',
       '**No new PGO was collected and no simulator was rebuilt.** All four simulator executables and both saved profiles match their pre-experiment SHA-256 hashes. PGO here means the original profiles collected from 12 Linux boots per simulator; this experiment measures how that existing optimization performs on CoreMark.',
       '## Runtime and MIPS',
       'Two warmup boots per variant, then ten measured boots per variant, interleaved in alternating forward/reverse order. Each process boots the same Linux image and runs CoreMark once. Runtime is host wall time between markers around CoreMark’s timed compute loop; firmware, boot, process initialization, result validation and shutdown are outside this interval.']
    body=[]
    for s,m in KEYS:
        v=[r['workload_seconds'] for r in groups[s,m]]
        body.append([s,m,len(v),f'{st.median(v):.6f}',f'{st.mean(v):.6f}',f'{st.stdev(v):.6f}',f'{min(v):.6f}',f'{max(v):.6f}',f'{mips[s,m]:.2f}'])
    t += [md_table(['Simulator','Build','N','Median s','Mean s','Std. dev. s','Min s','Max s','MIPS¹'],body),
          md_table(['Simulator','Baseline / PGO runtime','Runtime reduction','MIPS change'],[[s,f'{med[s,"baseline"]/med[s,"pgo"]:.3f}×',f'{100*(1-med[s,"pgo"]/med[s,"baseline"]):+.2f}%',f'{100*(mips[s,"pgo"]/mips[s,"baseline"]-1):+.2f}%'] for s in ['spike','qemu']]),
          f'Without PGO, QEMU is {med["spike","baseline"]/med["qemu","baseline"]:.2f}× faster than Spike on this workload.',
          '¹ Spike MIPS uses the guest `instret` delta from the same timed run. QEMU MIPS is an estimate using the median instruction count from three separately instrumented runs divided by the median normal workload time. Both cover the compute interval, including intervening kernel/firmware execution. These are guest instructions per host second, not host CPU MIPS.',
          '## Workload and timing method',
          f'- CoreMark upstream revision: `{rev}` in the [coremark](coremark) submodule. The algorithm and main-program source files are unmodified.\n- Single context, standard 2,000-byte data allocation, seeds `0, 0, 0x66`, and **{iterations:,} fixed iterations**. The same statically linked guest executable is used on all four simulator builds.\n- Guest compiler: GCC 13.3.0, `-O3 -static -march=rv64gc_zba_zbb_zbs_zfhmin -mabi=lp64d`. Guest code has no PGO.\n- [Timing hooks](configs/coremark/timing.c) wrap the port’s `start_time` and `stop_time` using the linker’s `--wrap` option. They emit flushed serial markers and two unique architectural NOPs to delimit instruction counting.\n- The host uses `perf_counter_ns()` and timestamps receipt of the start/end markers. There is no loop output. Small console-delivery and port-hook costs remain at the boundaries. Reported guest-clock time and the guest’s printed iterations/second are not used for comparison.\n- Iterations are embedded in the init script, avoiding console input and guest-clock automatic calibration. Calibration at 1,000 iterations selected 32,000 to target approximately three seconds on the fastest binary. Every variant executes the same amount of benchmark work.\n- The original one-hart RVA22S64 setup, 256 MiB RAM, Sv39, CLINT and HTIF remains in use. Linux 6.12.47, BusyBox 1.37.0 and OpenSBI 1.7 are built in the same Docker builder. The new image lives in `guest/coremark/`, preserving the original boot-only image.',
          '**This is a CoreMark workload runtime comparison, not an official CoreMark score.** Runs are deliberately shorter than the official minimum on QEMU, and guest clocks do not measure simulator wall-clock performance. The upstream duration warning remains in the logs; the harness accepts that specific warning, independently checks the seed/list/matrix/state CRCs, verifies the iteration count and process exit, and rejects other `ERROR!` messages. All final CRCs must also agree across variants. [CoreMark run rules](https://github.com/eembc/coremark#run-rules).',
          '## Instruction counting',
          'Spike exposes a retired-instruction counter through `rdinstret`; it is sampled twice by the port hooks in every timed run. Histogram mode is not used. QEMU’s normal `rdinstret` value is time-derived, so it is ignored. The separate [QEMU plugin](scripts/coremark-count.c) increments an inline counter and takes snapshots at the two marker NOPs; the harness requires exactly one start and one end. Normal QEMU timing runs do not load this plugin.',
          'QEMU’s callbacks count instruction dispatches before execution, including instructions that subsequently trap; Spike counts retirement. The marker/read boundaries also differ by a few instructions. Timer and interrupt activity can vary, so QEMU’s companion-count MIPS is an estimate rather than an exact per-run retirement rate. Counts include all privilege modes within the interval.']
    body=[]
    for s,m in KEYS:
        source=groups[s,m] if s=='spike' else counts[m]
        n=[r['instructions'] for r in source]
        body.append([s,m,f'{st.median(n):,.0f}',f'{min(n):,}',f'{max(n):,}', 'same timed runs' if s=='spike' else 'separate plugin runs'])
    outliers=[r for r in timing if not r['warmup'] and r['workload_seconds'] > 1.5*med[r['simulator'],r['mode']]]
    if outliers:
        t += ['Slow samples were retained without trimming: '+', '.join(f'{r["simulator"]} {r["mode"]} run {r["index"]}: {r["workload_seconds"]:.6f} s' for r in outliers)+'. Their cause was not established. The comparison uses medians, and the table includes the full spread.']
    t += [md_table(['Simulator','Build','Median instructions','Min','Max','Source'],body),
          md_table(['QEMU build','Counted workload median s','Plugin / normal runtime','Median counted-run MIPS'],[[m,f'{st.median(r["workload_seconds"] for r in counts[m]):.6f}',f'{st.median(r["workload_seconds"] for r in counts[m])/med["qemu",m]:.3f}×',f'{st.median(r["instructions"]/r["workload_seconds"]/1e6 for r in counts[m]):.2f}'] for m in ['baseline','pgo']]),
          '## All timing samples',
          'Runs −2 and −1 are warmups, excluded from summary statistics. Boot time ends at the BusyBox marker; process time includes boot, CoreMark, validation, shutdown and simulator teardown. [Raw JSONL](results/coremark/measurements.jsonl) includes full commands, timestamps and log paths.',
          md_table(['Simulator','Build','Run','Workload s','Boot s','Process s','Spike retired instructions'],[[r['simulator'],r['mode'],r['index'],f'{r["workload_seconds"]:.6f}',f'{r["boot_seconds"]:.6f}',f'{r["process_seconds"]:.6f}', f'{r["instructions"]:,}' if r['instructions'] else '—'] for r in timing]),
          '## All QEMU instruction-count samples',
          '[Raw JSONL](results/coremark/instruction-counts.jsonl). These instrumented runs are excluded from normal runtime statistics.',
          md_table(['Build','Run','Instructions','Workload s','Boot s','Process s'],[[r['mode'],r['index'],f'{r["instructions"]:,}',f'{r["workload_seconds"]:.6f}',f'{r["boot_seconds"]:.6f}',f'{r["process_seconds"]:.6f}'] for r in counting]),
          '## Calibration',
          'The successful fixed-input 1,000-iteration calibration is excluded from final statistics. The initial console-input attempt had dropped parameters on QEMU, entered automatic calibration and terminated with SIGILL; its partial logs are retained but are not valid performance samples. The final recipe uses no console input. Final measurements run with normal native host access, outside the restricted tool sandbox.',
          md_table(['Simulator','Build','Iterations','Workload s'],[[r['simulator'],r['mode'],r['iterations'],f'{r["workload_seconds"]:.6f}'] for r in read('calibration-fixed-1000.jsonl')]),
          '## Provenance and reproduction',
          'Host remains the Apple M5 with 32 GiB RAM and macOS 26.6.2. Native simulators remain LLVM 20.1.8 `-O3 -flto=thin` builds. [Host state for this experiment](results/coremark/host.json). Builds finished before the final timing/count series. No CPU pinning or fixed-frequency control was used; scheduler, thermal and background-process variation remain possible.',
          '[Unchanged simulator/profile hashes](results/coremark-existing-binaries-profiles.sha256), [guest config](results/coremark/linux.config), [workload parameters](results/coremark/workload.json), [guest build log](results/coremark/guest-build-32000.log), [guest compiler](results/coremark/guest-compiler.txt).',
          md_table(['Artifact','Bytes','SHA-256'],[[str(p.relative_to(ROOT)),p.stat().st_size,digest(p)] for p in [ROOT/'guest/coremark/Image',ROOT/'guest/coremark/fw_payload.elf',ROOT/'guest/coremark/coremark']]),
          'Reproduction commands are in [README.md](README.md#coremark-using-the-existing-pgo-builds). The [original boot-only report](REPORT.md) is preserved. These results characterize one scalar integer compute workload; they do not establish a universal ranking for floating-point, vector or memory-heavy applications.']
    (ROOT/'COREMARK_REPORT.md').write_text('\n\n'.join(t)+'\n')
    print(t[1])
if __name__=='__main__': main()
