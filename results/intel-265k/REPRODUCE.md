# Reproducing the Intel Core Ultra 7 265K run

Run from the repository root. Native builds use Ubuntu Clang 21.1.8 and LLD 21;
the guest builder uses Ubuntu 24.04's RISC-V GCC 13.3.0. No PGO stage is used.
Builds and fixtures must finish before starting timing. Use a new measurement tag
when repeating the experiment; the JSONL runners refuse to overwrite files.

## Native builds

The host already supplied Clang, LLD, Ninja, Python, pkg-config, dtc, GLib and
Pixman development packages. Boost was downloaded and extracted locally:

```sh
mkdir -p build/deps results/intel-265k
(cd build/deps && apt download libboost1.90-dev libboost-regex1.90-dev libboost-regex1.90.0)
for package in build/deps/*.deb; do dpkg-deb -x "$package" build/deps/root; done
bash scripts/build-linux-spike.sh
mkdir -p build/qemu-baseline
(
  cd build/qemu-baseline
  ../../qemu/configure --target-list=riscv64-softmmu --cc=clang --cxx=clang++ \
    --extra-cflags='-O3 -flto=thin' \
    --extra-ldflags='-O3 -flto=thin -fuse-ld=lld-21' \
    --enable-lto --disable-werror --disable-docs --disable-tools --disable-user \
    --disable-guest-agent --disable-slirp --disable-capstone --disable-gtk \
    --disable-sdl --disable-cocoa --enable-plugins --disable-gio
  ninja -j12 qemu-system-riscv64
)
clang -O3 -shared -fPIC $(pkg-config --cflags glib-2.0) -I qemu/include/plugins \
  scripts/coremark-count.c -o build/libcoremark-count.dylib $(pkg-config --libs glib-2.0)
bash scripts/test-linux-spike.sh
```

The plugin is an ELF shared object; its `.dylib` filename is retained because the
existing benchmark runner uses that path. Spike source archives are extracted
from the three recorded source commits into `build/sources/`, leaving the submodule's
existing checkout and staged state intact. Build flags and binary hashes are
written locally and are not committed.

## Guest images

Use a separate container workspace so that the scripts preserve the Apple M5
guest metadata in `results/`. Docker runs as the invoking user and writes only
to the two mounted build directories:

```sh
mkdir -p build/linux-guest-work/scripts build/linux-guest-work/configs build/guest-build
cp scripts/build-guest.sh scripts/build-coremark.sh build/linux-guest-work/scripts/
cp -a configs/coremark build/linux-guest-work/configs/
cp -a coremark build/linux-guest-work/
docker build -t qemu-vs-spike-guest:ubuntu24 -f configs/Dockerfile .
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$PWD/build/linux-guest-work:/work" -v "$PWD/build/guest-build:/build" \
  qemu-vs-spike-guest:ubuntu24 bash /work/scripts/build-guest.sh
mkdir -p guest results/intel-265k/guest-boot
cp -a build/linux-guest-work/guest/. guest/
cp -a build/linux-guest-work/results/. results/intel-265k/guest-boot/
docker run --rm --user "$(id -u):$(id -g)" \
  -e WORKLOAD=coremark -e COREMARK_ITERATIONS=32000 -e COREMARK_SEED=0 \
  -v "$PWD/build/linux-guest-work:/work" -v "$PWD/build/guest-build:/build" \
  qemu-vs-spike-guest:ubuntu24 bash /work/scripts/build-guest.sh
mkdir -p guest/coremark results/intel-265k/guest-coremark
cp -a build/linux-guest-work/guest/coremark/. guest/coremark/
cp -a build/linux-guest-work/results/coremark/. results/intel-265k/guest-coremark/
build/qemu-baseline/qemu-system-riscv64 \
  -M spike,dumpdtb=results/intel-265k/qemu-platform.dtb -cpu rva22s64 \
  -smp 1 -m 256M -nographic -bios guest/fw_payload.elf
dtc -I dtb -O dts results/intel-265k/qemu-platform.dtb -o results/intel-265k/qemu-platform.dts
diff -u results/qemu-platform.dts results/intel-265k/qemu-platform.dts
dtc -I dts -O dtb results/platform.dts -o results/platform.dtb
```

The generated device tree matched the original, so the timing runner uses the
existing `results/platform.dtb` with its translated Spike ISA string. The rebuilt
CoreMark executable also matched the original SHA-256. The Linux configurations
matched exactly; BusyBox settings matched with only the generated date comment
differing. Complete firmware/kernel hashes differ and are recorded separately.

## Timing

CPU 0 is a performance core on this host (`/sys/devices/cpu_core/cpus` is `0-7`).
The runners and child simulators inherit its affinity. Frequency control remains
at the host defaults: `intel_pstate`, `powersave`, energy preference `performance`.

```sh
taskset -c 0 python3 scripts/benchmark-optimization.py --tag intel-265k \
  --workload boot --runs 20 --warmups 2 \
  --variants spike:baseline spike:opt spike:opt3 qemu:baseline
taskset -c 0 python3 scripts/benchmark-optimization.py --tag intel-265k \
  --workload coremark --runs 10 --warmups 2 \
  --variants spike:baseline spike:opt spike:opt3 qemu:baseline
taskset -c 0 python3 scripts/benchmark-coremark.py --iterations 32000 \
  --simulators qemu --modes baseline --counted --runs 3 --warmups 0 \
  --output results/intel-265k/intel-265k-qemu-counts.jsonl
python3 scripts/summarize-series.py results/optimization/intel-265k-boot.jsonl \
  results/optimization/intel-265k-coremark.jsonl
```

The host was running its normal desktop applications. No benchmark build,
fixture compilation, profiler or counting run overlapped the primary timing
series. All outliers are retained; this is not an isolated or fixed-frequency host.
