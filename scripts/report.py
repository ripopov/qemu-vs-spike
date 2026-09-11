#!/usr/bin/env python3
"""Generate the Markdown report from successful recorded runs, never estimates of missing runs."""
import hashlib, json, statistics as st
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def rows(name): return [json.loads(l) for l in (ROOT/'results'/name).read_text().splitlines() if l]
def md_table(headers, body):
    return '\n'.join(['| '+' | '.join(headers)+' |','| '+' | '.join(['---']*len(headers))+' |']+['| '+' | '.join(map(str,r))+' |' for r in body])
def median(group, key): return st.median(r[key] for r in group)
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    timings=rows('measurements.jsonl'); counted=rows('instruction-counts.jsonl'); training=rows('training.jsonl')
    groups={(s,m):[r for r in timings if r['simulator']==s and r['mode']==m and not r['warmup']] for s in ('spike','qemu') for m in ('baseline','pgo')}
    counts={(s,m):[r for r in counted if r['simulator']==s and r['mode']==m and not r['warmup']] for s,m in groups}
    assert all(len(g)==20 for g in groups.values()), 'Need exactly 20 successful timed samples per variant'
    assert all(len(g)==5 for g in counts.values()), 'Need exactly 5 counted samples per variant'
    for r in timings+counted:
        assert (ROOT/r['log']).exists()
        assert b'BENCH_BUSYBOX_READY' in (ROOT/r['log']).read_bytes()
    boot={k:median(v,'boot_seconds') for k,v in groups.items()}
    winner=min(('spike','qemu'),key=lambda s:boot[s,'pgo'])
    ratio=boot['spike','pgo']/boot['qemu','pgo']
    host=json.loads((ROOT/'results/host.json').read_text())
    text=['# Spike versus QEMU: Linux boot on Apple M5',
          'A longer post-boot workload using the same simulator binaries is reported in [COREMARK_REPORT.md](COREMARK_REPORT.md).',
          f'**{winner.upper()} is fastest for this measured workload.** With PGO, QEMU reaches BusyBox in **{boot["qemu","pgo"]*1000:.2f} ms**, versus **{boot["spike","pgo"]*1000:.2f} ms** for Spike: **{ratio:.2f}×** faster by median wall-clock boot latency. This conclusion is specific to the single-hart minimal Linux boot below.',
          '## Boot-time results',
          '20 measured boots per binary, preceded by 3 warmups. All four variants were interleaved, reversing order every round. Values include native process startup, firmware and kernel initialization, and starting BusyBox ash as PID 1. The endpoint is receipt of `BENCH_BUSYBOX_READY`, printed by the BusyBox echo applet invoked by that shell. Guest poweroff and simulator teardown are excluded from boot latency.']
    body=[]
    for (s,m),g in groups.items():
        v=[r['boot_seconds']*1000 for r in g]
        body.append([s,m,len(g),f'{st.median(v):.3f}',f'{st.mean(v):.3f}',f'{st.stdev(v):.3f}',f'{min(v):.3f}',f'{max(v):.3f}'])
    text += [md_table(['Simulator','Build','N','Median ms','Mean ms','Std. dev. ms','Min ms','Max ms'],body)]
    text += [md_table(['Simulator','PGO boot speedup','Boot-time reduction'],[[s,f'{boot[s,"baseline"]/boot[s,"pgo"]:.3f}×',f'{100*(1-boot[s,"pgo"]/boot[s,"baseline"]):.2f}%'] for s in ('spike','qemu')]),f'Before PGO, QEMU is {boot["spike","baseline"]/boot["qemu","baseline"]:.2f}× faster than Spike by median boot latency.']
    text += ['## Instruction throughput (MIPS)',
    '**These are guest-instruction throughput estimates for the full boot-to-poweroff process, not native host MIPS or steady-state CPU benchmark scores.** Counting is performed in separate companion runs so the main boot timings remain uninstrumented. Counts cover firmware, kernel, BusyBox and shutdown; the corresponding time denominator is process launch through exit, not the earlier BusyBox marker.',
    '`Normalized MIPS = median(companion-run instruction count) / median(uninstrumented process seconds) / 1,000,000`.',
    'This removes counting overhead from the denominator, but is an estimate because timer-driven execution can change the instruction count between runs. Do not interpret it as an exact retired-instruction rate for an individual uninstrumented boot. The counted-run rate below uses count and elapsed time from the same instrumented run and includes instrumentation and output overhead.',
    'Spike uses upstream `-g` PC histograms, summed at normal exit. This forces its slow execution path; serializing instructions can be counted on more than one dispatch, and trapping instructions are not necessarily counted. QEMU uses [the local inline instruction-count plugin](scripts/insn-count.c), based on its supported plugin API; callbacks run before execution and include instructions that subsequently trap. The two counters therefore are not identical architectural retirement counters. Counting overhead and differing timer behavior make MIPS less suitable than boot latency for deciding which simulator boots this image fastest.']
    body=[]; normalized={}
    for key,g in groups.items():
        c=counts[key]; n=median(c,'instructions'); elapsed=median(g,'process_seconds'); normalized[key]=n/elapsed/1e6
        cmips=st.median(r['instructions']/r['process_seconds']/1e6 for r in c)
        body.append([*key,f'{n:,.0f}',f'{elapsed*1000:.3f}',f'{normalized[key]:.2f}',f'{cmips:.2f}',f'{median(c,"process_seconds")/elapsed:.2f}×'])
    text += [md_table(['Simulator','Build','Median counted instructions','Uninstrumented process ms','Normalized MIPS (estimate)','Counted-run MIPS','Counting time / normal time'],body)]
    text += [md_table(['Simulator','Build','Min instructions','Max instructions'], [[*k, f'{min(r["instructions"] for r in c):,}', f'{max(r["instructions"] for r in c):,}'] for k,c in counts.items()]), 'Spike’s histogram mode gets slower after PGO even though its normal boot gets faster: the training workload exercises its fast path, while histogram collection forces the slow path. The histogram timings therefore should not be used to rank the normal simulator binaries.']
    text += [md_table(['Simulator','Normalized MIPS before PGO','Normalized MIPS after PGO','Change'],[[s,f'{normalized[s,"baseline"]:.2f}',f'{normalized[s,"pgo"]:.2f}',f'{100*(normalized[s,"pgo"]/normalized[s,"baseline"]-1):+.2f}%'] for s in ('spike','qemu')])]
    text += ['## Host, sources and build',
    '- Host: Apple M5, 32 GiB RAM, 4 performance cores + 6 efficiency cores. Native arm64 macOS 26.6.2 (25G83). AC power; low-power mode disabled. No CPU pinning or fixed-frequency control; the macOS scheduler chooses the host core.\n- Host compiler: Homebrew Clang/LLVM 20.1.8. Both baseline and PGO builds use `-O3 -flto=thin` at compilation and linking. Earlier `-O2` flags emitted by the projects are overridden by the final `-O3`. ThinLTO is LLVM link-time optimization; `-lto` is not the Clang option. Shared Homebrew dependencies were not rebuilt or PGO-trained.\n- PGO: `-fprofile-instr-generate` training binaries, 12 full boots per simulator, separate per-simulator `llvm-profdata merge`, then fresh builds with `-fprofile-instr-use`. Training and evaluation use the same workload; results show workload-specific optimization rather than performance on unseen workloads. No instrumentation or profile-writing flags are used in the final timed binaries.\n- All guest compilation ran in an arm64 Ubuntu 24.04 Docker container using GCC 13.3.0 / binutils 2.42. Simulator builds and measurements ran natively on the Mac. Compilation was finished before the reported measurement series. Docker Hub metadata retrieval stalled, so the cached builder was used. Guest compilation used a Docker volume after tar extraction failed on the macOS bind mount.',
    f'Builder image: `{host["builder"]["Id"]}`. Full host metadata: [host.json](results/host.json). Guest compiler: [guest-compiler.txt](results/guest-compiler.txt). Actual build flags: [build-flags.json](results/build-flags.json). Binary hashes: [native-sha256.txt](results/native-sha256.txt).',
    'The submodules pin the latest upstream default-branch HEAD returned at checkout on 2026-09-10, rather than the latest tagged releases:',
    md_table(['Component','Revision / version'],[
        ['Spike','`4ffd6ba860f4190ceac2716fa3c2cf139e85538f`'],['QEMU','`257bf4f160c50ca8c4ebd603f519f5c786013fb7`'],['Linux','6.12.47'],['BusyBox','1.37.0'],['OpenSBI','1.7']]),
    'The Spike and QEMU submodule source trees are unmodified. The counting plugin lives outside QEMU. All scripts, guest configs, raw logs and samples are in this repository; builds and downloaded images are ignored by Git. See [README.md](README.md) for reproduction commands.',
    'PGO validation: both merged profiles contain nonzero execution counts (18,773 recorded Spike functions and 39,272 QEMU functions). The QEMU build emitted 39 profile-shape mismatch warnings while compiling alternate stub implementations, whose mismatching data Clang ignored; unused or unlinked files also reported missing profiles. These diagnostics are preserved in [native-pgo-build.log](results/native-pgo-build.log). The final binaries build and boot successfully; this does not imply every compiled function received usable profile data.',
    '## Guest and platform',
    '- One RV64 hart, 256 MiB RAM at `0x80000000`, Sv39, 10 MHz CLINT, HTIF console and poweroff; no disks, networking or graphical display. QEMU uses the Spike machine with single-threaded TCG, without hardware virtualization.\n- QEMU selects `rva22s64`. Spike enables the matching implemented mandatory ISA extensions explicitly. Its device tree derives from QEMU’s, with only the legacy `riscv,isa` string translated because Spike rejects several QEMU descriptive feature names. The extension-list property and device/memory topology are preserved. This is an RVA22S64-targeted configuration, not a profile-conformance test.\n- Linux starts from `tinyconfig`, with MMU, SBI, ELF/script loading, static built-in initramfs and console support. SMP is disabled. The kernel uses its normal RV64 compiler ISA baseline and selected extension support; it does not force every guest instruction to use RVA22-specific extensions. BusyBox is statically linked with only the shell, echo and halt/poweroff functionality needed here.\n- Both simulators load the identical OpenSBI payload ELF, with the identical embedded Linux Image and initramfs. Firmware is linked at `0x80000000`; Linux starts at `0x80200000`.\n- Kernel command line: `console=hvc0 earlycon=sbi rdinit=/init nokaslr lpj=1000000 quiet`. Fixed `lpj` avoids timer-based delay calibration; it is a benchmark choice, not a calibrated real-time delay setting.\n- Spike’s CLINT advances with simulated instruction progress by default; QEMU’s uses its virtual clock tied to host execution. This difference is retained in the normal fast configurations and can affect scheduling, idle paths and instruction counts. QEMU runs without `-icount`.\n- OpenSBI and shutdown console output remain enabled and are captured through a pipe for every run. The host-visible marker includes console-delivery latency. This is boot to a functioning BusyBox shell, not an interactive login prompt or a steady-state userspace workload.',
    'Configs: [Linux](results/linux.config), [BusyBox](results/busybox.config), [QEMU device tree](results/qemu-platform.dts), [Spike device tree](results/platform.dts).',
    md_table(['Artifact','Bytes','SHA-256'],[[str(p.relative_to(ROOT)),p.stat().st_size,sha(p)] for p in [ROOT/'guest/Image',ROOT/'guest/fw_payload.elf',ROOT/'results/platform.dtb']]),
    '## Every measured boot',
    'Milliseconds to the BusyBox marker. Warmups are listed separately below. Raw records include full commands, timestamps and paths to captured output: [measurements.jsonl](results/measurements.jsonl).']
    keys=[('spike','baseline'),('qemu','baseline'),('spike','pgo'),('qemu','pgo')]
    def grid(rs,field):
        indices=sorted(set(r['index'] for r in rs))
        lookup={(r['simulator'],r['mode'],r['index']):r for r in rs}
        return md_table(['Run']+[f'{s} {m}' for s,m in keys],[[i]+[f'{lookup[s,m,i][field]*1000:.3f}' for s,m in keys] for i in indices])
    text += [grid([r for r in timings if not r['warmup']],'boot_seconds'), '### Full-process times for those boots (ms)',grid([r for r in timings if not r['warmup']],'process_seconds'), '### Warmup boot times (ms)',grid([r for r in timings if r['warmup']],'boot_seconds'), '### Warmup full-process times (ms)',grid([r for r in timings if r['warmup']],'process_seconds')]
    text += ['## Every instruction-count run','One warmup per variant, then five samples. Run −1 is the warmup and is excluded from summary statistics. Full raw data: [instruction-counts.jsonl](results/instruction-counts.jsonl).',md_table(['Simulator','Build','Run','Instructions','Boot ms','Process ms','Counted MIPS'],[[r['simulator'],r['mode'],r['index'],f'{r["instructions"]:,}',f'{r["boot_seconds"]*1000:.3f}',f'{r["process_seconds"]*1000:.3f}',f'{r["instructions"]/r["process_seconds"]/1e6:.2f}'] for r in counted])]
    text += ['## PGO training measurements','Instrumentation makes these binaries slower; these times are excluded from performance comparisons. All 12 samples per simulator contributed to the corresponding profile. [Raw training data](results/training.jsonl).',md_table(['Simulator','Run','Boot ms','Process ms'],[[r['simulator'],r['index'],f'{r["boot_seconds"]*1000:.3f}',f'{r["process_seconds"]*1000:.3f}'] for r in training]),'## Scope and references',
    'Repeated fresh processes use warm filesystem caches; these are not cold-disk or machine-reboot measurements. There is no correction for host background activity, thermal state or core migration. The result compares the two selected simulator configurations on one small boot workload and should not be generalized to vector, floating-point, multicore or long-running applications.',
    'Preliminary smoke and diagnostic runs occurred before the final series, some during compilation, and are excluded from the above statistics. Their available records remain in [smoke.jsonl](results/smoke.jsonl), [counts.jsonl](results/counts.jsonl) and [diagnostic-counts.jsonl](results/diagnostic-counts.jsonl). Failed configuration, build and boot attempts produced no valid benchmark sample. The final series requires both the marker and a successful process exit for every sample.',
    '- [Spike upstream source and documentation](https://github.com/riscv-software-src/riscv-isa-sim/tree/4ffd6ba860f4190ceac2716fa3c2cf139e85538f) — interpreter, ISA options and histogram behavior.\n- [QEMU emulation and instruction-count plugins](https://www.qemu.org/docs/master/about/emulation.html) — instruction-count instrumentation.\n- [QEMU TCG instruction counting](https://www.qemu.org/docs/master/devel/tcg-icount.html) — why `-icount` changes execution and timer behavior.']
    preliminary = []
    for name in ['smoke.jsonl', 'counts.jsonl', 'diagnostic-counts.jsonl']:
        for r in rows(name):
            preliminary.append([name,r['simulator'],r['mode'],r['index'],f'{r["boot_seconds"]*1000:.3f}',f'{r["process_seconds"]*1000:.3f}',r['instructions'] or '—'])
    text += ['## Preliminary measurements (excluded)', 'These successful setup checks were taken before the final timing series. Some overlapped builds. Early process timings also include the small cost of writing captured logs; the final series timestamps process exit before writing logs. They are retained for completeness and are not used in any summary.', md_table(['Data file','Simulator','Build','Run','Boot ms','Process ms','Instructions'], preliminary)]
    (ROOT/'REPORT.md').write_text('\n\n'.join(text)+'\n')
    print(text[1])
if __name__=='__main__': main()
