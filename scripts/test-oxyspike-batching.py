#!/usr/bin/env python3
"""Compare CLI checkpoints with a preserved per-instruction OxySpike build."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference', type=Path)
parser.add_argument('candidate', type=Path)
parser.add_argument('--output', type=Path, default=Path('results/oxyspike/batching-checkpoints.json'))
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
current = args.candidate.resolve()
assert hashlib.sha256(current.read_bytes()).digest() != hashlib.sha256(args.reference.read_bytes()).digest(), 'Choose distinct builds'
limits = [0, 1, 2, 98, 99, 100, 101, 102, 198, 199, 200, 201,
          999, 1000, 1001, 10000, 10001, 100000, 100001,
          1000000, 1000001, 10000000, 30000000, 70000000]
for limit in limits:
    outputs = []
    for binary in [args.reference.resolve(), current]:
        result = subprocess.run([str(binary), '--trace-traps', '--dtb',
                                 'results/platform.dtb', '--max-instructions', str(limit),
                                 'guest/fw_payload.elf'], cwd=root, capture_output=True, timeout=30)
        assert result.returncode == 1, (binary, limit, result.returncode)
        outputs.append((result.stdout, result.stderr))
    assert outputs[0] == outputs[1], f'CLI state or trap trace differs at {limit} attempts'
summary = {'limits': limits, 'mismatches': 0,
           'compared': 'stdout, full trap trace, final PC/instret/ra/sp/a0/time and exit status',
           'sha256': {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in [args.reference, current]}}
(root / args.output).write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
