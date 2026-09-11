#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build/oxyspike-tests results/oxyspike
spike_source="${SPIKE_SOURCE:-build/sources/spike-baseline}"
"${CXX:-c++}" -std=c++17 -O2 -I "$spike_source/riscv" -I build/spike-baseline \
  oxyspike/tests/compressed-reference.cc -o build/oxyspike-tests/compressed-reference
build/oxyspike-tests/compressed-reference > build/oxyspike-tests/compressed-reference.txt
cargo run --offline --locked --release --manifest-path oxyspike/Cargo.toml \
  --example compressed-vectors > build/oxyspike-tests/compressed-rust.txt
python3 - "$spike_source" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

ref = Path('build/oxyspike-tests/compressed-reference.txt')
rust = Path('build/oxyspike-tests/compressed-rust.txt')
a, b = ref.read_text().splitlines(), rust.read_text().splitlines()
assert len(a) == len(b) == 65536
mismatches = [(i, x, y) for i, (x, y) in enumerate(zip(a, b)) if x != y]
assert not mismatches, mismatches[:20]
assert all(line.split()[0] == f'{i:04x}' for i, line in enumerate(b))
expanded = sum(line.split()[1] != '-' for line in b)
paths = [Path(sys.argv[1]) / 'riscv/decode.h',
         Path('oxyspike/src/compressed.rs'), Path('oxyspike/tests/compressed-reference.cc'),
         Path('oxyspike/examples/compressed-vectors.rs'),
         Path('scripts/test-oxyspike-compressed.sh'),
         Path('build/oxyspike-tests/compressed-reference'),
         Path('oxyspike/target/release/examples/compressed-vectors'), ref, rust]
summary = {
    'encodings': 65536, 'expanded': expanded, 'rejected_or_not_compressed': 65536-expanded,
    'mismatches': 0,
    'scope': 'Canonical expansion comparison using Spike immediate/register extractors and a validation-only RV64C mapping; not exhaustive execution conformance or an independent ISA legality oracle.',
    'sha256': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
}
Path('results/oxyspike/compressed-validation.json').write_text(json.dumps(summary, indent=2)+'\n')
print(json.dumps(summary, indent=2))
PY
