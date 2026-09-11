#!/usr/bin/env python3
"""Verify that dispatch profiling preserves CLI checkpoints and emits valid counts."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference', type=Path)
parser.add_argument('profiled', type=Path)
parser.add_argument('--output', type=Path, default=Path('results/oxyspike/dispatch-profile-checkpoints.json'))
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
limits = [0, 1, 2, 98, 99, 100, 101, 102, 198, 199, 200, 201,
          999, 1000, 1001, 10000, 10001, 100000, 100001,
          1000000, 1000001, 10000000, 30000000, 70000000]
for limit in limits:
    outputs = []
    for binary in [args.reference, args.profiled]:
        result = subprocess.run([str(binary.resolve()), '--trace-traps', '--dtb',
                                 'results/platform.dtb', '--max-instructions', str(limit),
                                 'guest/fw_payload.elf'], cwd=root, capture_output=True, timeout=30)
        assert result.returncode == 1, (binary, limit, result.returncode)
        outputs.append(result)
    lines = outputs[1].stderr.splitlines(keepends=True)
    profile_lines = [line for line in lines if line.startswith(b'OXY_DISPATCH_PROFILE ')]
    assert len(profile_lines) == 1, limit
    p = json.loads(profile_lines[0].removeprefix(b'OXY_DISPATCH_PROFILE '))
    assert 0 <= p['misses'] <= p['lookups'] <= limit, limit
    assert sum(p['operations'].values()) <= p['lookups'], limit
    assert len(p['slow_opcodes']) == 128, limit
    assert sum(p['slow_opcodes']) == p['operations']['Slow'], limit
    assert len(p['slow_functions']) == 4096, limit
    for group, opcode in enumerate([0x13, 0x1b, 0x33, 0x3b]):
        assert sum(p['slow_functions'][group*1024:(group+1)*1024]) == p['slow_opcodes'][opcode], limit
    assert all(isinstance(v, int) and v >= 0 for v in [*p['operations'].values(), *p['slow_opcodes']]), limit
    stderr = b''.join(line for line in lines if not line.startswith(b'OXY_DISPATCH_PROFILE '))
    assert outputs[0].stdout == outputs[1].stdout, limit
    assert outputs[0].stderr == stderr, limit
summary = {'limits': limits, 'mismatches': 0,
           'compared': 'stdout, full trap trace, final CLI state and exit status; only the explicit profiling line is excluded',
           'sha256': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [args.reference, args.profiled]}}
(root / args.output).write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
