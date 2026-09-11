#!/usr/bin/env python3
"""Exhaustive half widening and seeded narrowing against Spike SoftFloat."""
import json
from pathlib import Path
import random
import struct
import subprocess

root = Path(__file__).resolve().parents[1]
rng = random.Random(0x16F00D)
cases = [(16, op, 0, a, 0, 0) for a in range(65536) for op in (5, 14)]
for fmt in (32, 64):
    # Half's finite endpoints, subnormal boundary, and rounding ties.
    pack = 'f' if fmt == 32 else 'd'
    values = [0.0, -0.0, 2**-25, 2**-24, 2**-14 - 2**-25,
              2**-14, 1 + 2**-11, 65504.0, 65520.0,
              float('inf'), float('-inf'), float('nan')]
    bits = [int.from_bytes(struct.pack('<' + pack, v), 'little') for v in values]
    bits += [x ^ (1 << (fmt - 1)) for x in bits]
    for rm in range(5):
        for a in bits + [rng.getrandbits(fmt) for _ in range(2000)]:
            cases.append((fmt, 14, rm, a, 0, 0))
data = ''.join(f'{f} {o} {r} {a:x} {b:x} {c:x}\n' for f, o, r, a, b, c in cases)
answers = []
for binary in ('build/oxyspike-tests/softfloat-reference',
               'oxyspike/target/release/examples/softfloat-check'):
    answers.append(subprocess.run([str(root / binary)], input=data, text=True,
                                  capture_output=True, check=True).stdout.splitlines())
reference, actual = answers
assert len(reference) == len(actual) == len(cases)
errors = [dict(case=c, reference=r, actual=a)
          for c, r, a in zip(cases, reference, actual) if r != a]
summary = dict(cases=len(cases), exhaustive_half_patterns=65536,
               mismatches=len(errors), examples=errors[:10])
print(json.dumps(summary, indent=2))
(root / 'results/oxyspike/half-validation.json').write_text(json.dumps(summary, indent=2) + '\n')
if errors:
    raise SystemExit(1)
vectors = bytearray()
for (fmt, op, rm, a, _, _), answer in zip(cases, reference):
    expected, flags = (int(answer.split()[0], 16), int(answer.split()[1]))
    src = {16: 2, 32: 0, 64: 1}[fmt]
    dst = (0 if op == 5 else 1) if fmt == 16 else 2
    insn = ((0x20 | dst) << 25) | (src << 20) | (1 << 15) | (rm << 12) | (28 << 7) | 0x53
    if fmt < 64:
        a |= ((1 << 64) - 1) ^ ((1 << fmt) - 1)
    if dst == 2:
        expected |= 0xffffffffffff0000
    vectors.extend(struct.pack('<5QIB3x', a, 0, 0, expected, flags, insn, 2 if dst == 0 else 0))
(root / 'build/oxyspike-tests/half-cases.bin').write_bytes(vectors)
# Reuse the established vector runner; half outputs are compared with their full boxing bits.
source = (root / 'oxyspike/tests/floating.S').read_text().replace('floating-cases.bin', 'half-cases.bin')
(root / 'build/oxyspike-tests/half-vectors.S').write_text(source)
