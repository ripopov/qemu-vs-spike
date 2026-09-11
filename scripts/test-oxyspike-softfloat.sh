#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build/oxyspike-tests
clang -O2 -fuse-ld=lld -Ibuild/sources/spike-baseline/softfloat \
    oxyspike/tests/softfloat-reference.c build/spike-baseline/libsoftfloat.a \
    -o build/oxyspike-tests/softfloat-reference
cargo build --offline --release --manifest-path oxyspike/Cargo.toml --examples
python3 scripts/test-oxyspike-softfloat.py
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imafdc_zicsr_zifencei -mabi=lp64d \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none oxyspike/tests/floating.S \
    -o build/oxyspike-tests/floating.elf
cargo build --offline --release --manifest-path oxyspike/Cargo.toml
oxyspike/target/release/oxyspike --max-instructions 3000000 build/oxyspike-tests/floating.elf
timeout 30 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imafdc_zicsr_zifencei build/oxyspike-tests/floating.elf
