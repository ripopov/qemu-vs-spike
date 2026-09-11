#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build/oxyspike-tests
clang -O2 -fuse-ld=lld -Ibuild/sources/spike-baseline/softfloat \
  oxyspike/tests/softfloat-reference.c build/spike-baseline/libsoftfloat.a \
  -o build/oxyspike-tests/softfloat-reference
cargo build --offline --release --manifest-path oxyspike/Cargo.toml --examples
python3 scripts/test-oxyspike-half.py
cargo build --offline --release --manifest-path oxyspike/Cargo.toml
for fixture in half half-vectors; do
  source="oxyspike/tests/$fixture.S"
  if [[ "$fixture" == half-vectors ]]; then source="build/oxyspike-tests/$fixture.S"; fi
  riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imafdc_zicsr_zifencei_zfhmin -mabi=lp64d \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none "$source" -o "build/oxyspike-tests/$fixture.elf"
  oxyspike/target/release/oxyspike --max-instructions 10000000 "build/oxyspike-tests/$fixture.elf"
  timeout 60 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" \
    --isa=rv64imafdc_zicsr_zifencei_zfhmin "build/oxyspike-tests/$fixture.elf"
done
