# Spike, OxySpike and QEMU/TCG

A comparison of RISC-V interpreters and QEMU's TCG binary translator on Apple M5
and Intel Core Ultra 7 265K. Each simulator boots Linux and runs the same
32,000-iteration CoreMark workload on a single RV64 hart with 256 MiB RAM.
[OxySpike](oxyspike/) is implemented in safe Rust; the optimized C++ Spike retains
an interpreter execution model.

## Final results

Host wall-time medians; lower times and higher MIPS are better. Linux boot ends at the BusyBox
`BENCH_BUSYBOX_READY` marker. CoreMark time is measured between guest console
markers around its timed loop, excluding boot. These are workload runtimes,
not official CoreMark scores.

| Host | Simulator | Linux boot | CoreMark | Guest MIPS | Measurement series |
| --- | --- | ---: | ---: | ---: | --- |
| Apple M5 | C++ Spike, optimized | 180.9 ms | 10.572 s | 919 | M5 |
| Apple M5 | QEMU/TCG | 157.0 ms | 3.500 s | 2,779 | M5 |
| Intel 265K | C++ Spike, optimized | 196.7 ms | 11.995 s | 810 | Intel comparison |
| Intel 265K | QEMU/TCG | 175.8 ms | 3.438 s | 2,827 | Intel comparison |
| Intel 265K | OxySpike, release | 350.5 ms | 31.927 s | 304 | Rust final |
| Intel 265K | OxySpike, Linux-trained PGO | 309.8 ms | 29.887 s | 325 | Rust final |

Guest MIPS is millions of guest instructions per second during CoreMark, computed
as the instruction count divided by the unrounded median runtime and 1,000,000.
Spike and OxySpike counts come from `instret`; QEMU uses median counts from separate
plugin-instrumented runs, combined with uninstrumented timings. QEMU MIPS is
therefore an estimate.

The Intel comparison and Rust final rows come from separate measurement sessions
on the same host. The Rust final series measures the current refactored source.
Its paired comparison with the preceding Rust build shows no measurable regression:
CoreMark medians differ by about 0.1%, within the observed run-to-run spread, and
Linux boot is slightly faster.

- **M5:** native macOS arm64, LLVM 20.1.8, `-O3 -flto=thin`, no PGO;
  20 measured boots and 10 CoreMark runs per build.
- **Intel comparison:** Ubuntu on Intel 265K, pinned to performance core 0;
  five measured boots after one warmup and five CoreMark runs without warmups.
- **Rust final:** same affinity and sample counts; Rust 1.95.0 / LLVM 22.1.2,
  release optimization level 3, fat LTO, one codegen unit. PGO trains on three
  Linux boots, with CoreMark held out.

Runs are serial, alternate build order, exclude declared warmups, and retain
outliers. Builds, profilers and correctness tests do not overlap timing runs.
The M5 and Intel guest firmware builds and host environments differ; compare
simulators within each host rather than interpreting the table as a CPU ranking.
The guest uses Linux 6.12.47, BusyBox and OpenSBI 1.7, with HTIF console/shutdown
and a CLINT timer. Generated logs, measurements, profiles and build artifacts are
kept locally and ignored by Git. This README retains the final reported results;
rerun the scripts below to generate fresh measurements and validation evidence.

## Implementations

The optimized [C++ Spike](spike/) preserves instruction caches across changes to
data-only permissions, separates caches by privilege regime, uses lazy invalidation
and address-specific fences, streamlines load/store fast paths, and reduces host
console polling. The final optimized source is commit `52bf14f`; native regression
fixtures are in [tests/](tests/).

[OxySpike](oxyspike/) uses decoded-instruction caching, fetch/load/store TLBs,
whole-page PMP permission checks, epoch-based instruction-cache invalidation and
event-driven interrupt polling. Exact integer-based floating-point arithmetic
avoids dependence on the host floating-point environment. Architectural constants
and short invariant comments document the cache, privilege, memory and timer paths.
Cargo enforces `unsafe_code = "forbid"`.

OxySpike boots the shared Linux image and validates CoreMark checksums, but full
Spike ISA/platform compatibility is not claimed. The measurements cover its
implemented subset; the shared device tree advertises some unimplemented extensions.

## Validation

The final Rust source passes 65 Rust tests, all-feature tests and Clippy with
warnings denied. Both ordinary and PGO binaries pass 972,132 integer cases with
register aliases and 24 Linux checkpoints; the PGO binary also passes all 16 small
guest fixtures. Floating-point validation covers 56,140 single/double and 151,312
half-precision reference cases. All 65,536 compressed encodings are checked against
the existing expansion reference, which is not an independent legality oracle.

All 20 final CoreMark runs have identical CRCs and retire 9,719,225,560 instructions
in the measured region. Checkpoints compare console output, trap traces, selected
registers, counters, timer and exit status, rather than full RAM/register state.

## Build and reproduce

Clone with submodules. Guest images and build directories are not committed.
For Linux simulator builds, guest preparation and host dependencies, follow the
[Intel reproduction instructions](results/intel-265k/REPRODUCE.md).

For Apple M5, install Docker Desktop, Xcode command-line tools, Homebrew `llvm@20`,
`boost`, `dtc`, `glib`, `pixman`, `pkgconf`, `ninja`, and Python 3, then run:

```sh
scripts/run-guest-build.sh
scripts/build-native.sh baseline
scripts/prepare-platform.sh
WORKLOAD=coremark COREMARK_ITERATIONS=32000 scripts/run-guest-build.sh
git -C spike checkout 52bf14f
bash scripts/build-spike-variant.sh opt3
```

Build and validate OxySpike with Rust and the RISC-V GNU toolchain installed.
The differential scripts also require the C++ Spike reference build:

```sh
cargo build --offline --locked --release --manifest-path oxyspike/Cargo.toml
bash scripts/test-oxyspike.sh
bash scripts/test-oxyspike-softfloat.sh
bash scripts/test-oxyspike-half.sh
bash scripts/test-oxyspike-compressed.sh
python3 scripts/test-oxyspike-integer.py --aliases --output results/oxyspike/my-integer.json
cargo clippy --offline --locked --all-targets --all-features \
  --manifest-path oxyspike/Cargo.toml -- -D warnings
python3 scripts/build-oxyspike-pgo.py my-pgo
```

The PGO binary is written to `build/oxyspike-variants/my-pgo`. With both guest
images and the platform device tree prepared, run the Intel comparison:

```sh
python3 scripts/benchmark-optimization.py --tag my-final --workload boot \
  --runs 5 --warmups 1 --variants spike:opt3 qemu:baseline oxyspike:release oxyspike:my-pgo
python3 scripts/benchmark-optimization.py --tag my-final --workload coremark \
  --runs 5 --warmups 0 --variants spike:opt3 qemu:baseline oxyspike:release oxyspike:my-pgo
```

Use fresh PGO variant names and measurement tags when repeating experiments;
the tools refuse to overwrite existing outputs. The optional `dispatch-profile`
Cargo feature records diagnostic operation/cache counts and should be disabled
for timing runs.
