#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --manifest-path oxyspike/Cargo.toml
cargo build --release --manifest-path oxyspike/Cargo.toml
mkdir -p build/oxyspike-tests
for isa in rv64ima rv64imac; do
    riscv64-linux-gnu-gcc -nostdlib -static -march="$isa" -mabi=lp64 \
        -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
        oxyspike/tests/smoke.S -o "build/oxyspike-tests/smoke-$isa.elf"
    oxyspike/target/release/oxyspike --max-instructions 10000 "build/oxyspike-tests/smoke-$isa.elf"
    timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa="$isa" "build/oxyspike-tests/smoke-$isa.elf"
done
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/privileged.S -o build/oxyspike-tests/privileged.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/privileged.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr build/oxyspike-tests/privileged.elf
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imafdc_zicsr_zba_zbb_zbs -mabi=lp64d \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/extensions.S -o build/oxyspike-tests/extensions.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/extensions.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imafdc_zicsr_zba_zbb_zbs build/oxyspike-tests/extensions.elf
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr_zicntr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/counters.S -o build/oxyspike-tests/counters.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/counters.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_zicntr build/oxyspike-tests/counters.elf
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr_svpbmt_svinval -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/vm-extensions.S -o build/oxyspike-tests/vm-extensions.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/vm-extensions.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_svpbmt_svinval build/oxyspike-tests/vm-extensions.elf
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/clint.S -o build/oxyspike-tests/clint.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/clint.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr build/oxyspike-tests/clint.elf
riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/fetch-length.S -o build/oxyspike-tests/fetch-length.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/fetch-length.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr build/oxyspike-tests/fetch-length.elf

riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/atomic-faults.S -o build/oxyspike-tests/atomic-faults.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/atomic-faults.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr build/oxyspike-tests/atomic-faults.elf

riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/split-store.S -o build/oxyspike-tests/split-store.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/split-store.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_zicclsm build/oxyspike-tests/split-store.elf

riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/split-load.S -o build/oxyspike-tests/split-load.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/split-load.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_zicclsm build/oxyspike-tests/split-load.elf

riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/hpm.S -o build/oxyspike-tests/hpm.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/hpm.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_zicntr_zihpm build/oxyspike-tests/hpm.elf

riscv64-linux-gnu-gcc -nostdlib -static -march=rv64imac_zicsr -mabi=lp64 \
    -Wl,-T,oxyspike/tests/link.ld -Wl,--build-id=none \
    oxyspike/tests/cbo.S -o build/oxyspike-tests/cbo.elf
oxyspike/target/release/oxyspike --max-instructions 10000 build/oxyspike-tests/cbo.elf
timeout 10 "${SPIKE_REFERENCE:-build/spike-baseline/spike}" --isa=rv64imac_zicsr_zicbom_zicboz build/oxyspike-tests/cbo.elf
