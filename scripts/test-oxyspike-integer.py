#!/usr/bin/env python3
"""Run deterministic RV64 M/Zba/Zbb/Zbs arithmetic vectors on Spike and OxySpike."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import subprocess

MASK = (1 << 64) - 1


def signed(value, width=64):
    value &= (1 << width) - 1
    return value - (1 << width) if value >> (width - 1) else value


def quotient(a, b):
    if b == 0:
        return -1
    q = abs(a) // abs(b)
    return -q if (a < 0) != (b < 0) else q


def rotate(a, b, width, left=False):
    a &= (1 << width) - 1
    b %= width
    if left:
        b = (-b) % width
    return ((a >> b) | (a << ((width - b) % width))) & ((1 << width) - 1)


def groups():
    out = []

    def reg(name, fn):
        out.append((name, f'{name} t2,t0,t1', fn))

    def unary(name, fn):
        out.append((name, f'{name} t2,t0', lambda a, b: fn(a)))

    for width, suffix in [(64, ''), (32, 'w')]:
        mask = (1 << width) - 1
        def wrap(fn, width=width):
            return lambda a, b: signed(fn(a, b), 32) if width == 32 else fn(a, b)
        reg('mul' + suffix, wrap(lambda a, b: a * b))
        reg('div' + suffix, wrap(lambda a, b, w=width: quotient(signed(a, w), signed(b, w))))
        reg('divu' + suffix, wrap(lambda a, b, m=mask: (a & m) // (b & m) if b & m else m))
        reg('rem' + suffix, wrap(lambda a, b, w=width: signed(a, w) if signed(b, w) == 0 else signed(a, w) - quotient(signed(a, w), signed(b, w)) * signed(b, w)))
        reg('remu' + suffix, wrap(lambda a, b, m=mask: (a & m) % (b & m) if b & m else a & m))
        for left, name in [(True, 'rol'), (False, 'ror')]:
            reg(name + suffix, wrap(lambda a, b, w=width, left=left: rotate(a, b, w, left)))
        for name, fn in [
            ('clz', lambda a, w=width: w - (a & ((1 << w) - 1)).bit_length()),
            ('ctz', lambda a, w=width: ((a & -a).bit_length() - 1) if a & ((1 << w) - 1) else w),
            ('cpop', lambda a, m=mask: (a & m).bit_count()),
        ]:
            unary(name + suffix, fn)
    reg('mulh', lambda a, b: (signed(a) * signed(b)) >> 64)
    reg('mulhu', lambda a, b: (a * b) >> 64)
    reg('mulhsu', lambda a, b: (signed(a) * b) >> 64)
    for shift in [1, 2, 3]:
        reg(f'sh{shift}add', lambda a, b, n=shift: (a << n) + b)
        reg(f'sh{shift}add.uw', lambda a, b, n=shift: ((a & 0xffffffff) << n) + b)
    reg('add.uw', lambda a, b: (a & 0xffffffff) + b)
    for name, fn in {
        'andn': lambda a, b: a & ~b,
        'orn': lambda a, b: a | ~b,
        'xnor': lambda a, b: ~(a ^ b),
        'min': lambda a, b: min(signed(a), signed(b)),
        'max': lambda a, b: max(signed(a), signed(b)),
        'minu': min, 'maxu': max,
        'bset': lambda a, b: a | (1 << (b & 63)),
        'bclr': lambda a, b: a & ~(1 << (b & 63)),
        'binv': lambda a, b: a ^ (1 << (b & 63)),
        'bext': lambda a, b: (a >> (b & 63)) & 1,
    }.items():
        reg(name, fn)
    unary('sext.b', lambda a: signed(a, 8))
    unary('sext.h', lambda a: signed(a, 16))
    unary('zext.h', lambda a: a & 65535)
    unary('rev8', lambda a: int.from_bytes(a.to_bytes(8, 'little'), 'big'))
    unary('orc.b', lambda a: sum(255 << n for n in range(0, 64, 8) if (a >> n) & 255))
    for shift in range(64):
        for name, fn in [
            ('rori', lambda a, b, n=shift: rotate(a, n, 64)),
            ('slli.uw', lambda a, b, n=shift: (a & 0xffffffff) << n),
            ('bseti', lambda a, b, n=shift: a | (1 << n)),
            ('bclri', lambda a, b, n=shift: a & ~(1 << n)),
            ('binvi', lambda a, b, n=shift: a ^ (1 << n)),
            ('bexti', lambda a, b, n=shift: (a >> n) & 1),
        ]:
            out.append((f'{name}-{shift}', f'{name} t2,t0,{shift}', fn))
        if shift < 32:
            out.append((f'roriw-{shift}', f'roriw t2,t0,{shift}', lambda a, b, n=shift: signed(rotate(a, n, 32), 32)))
    return out



def register_layouts(aliases):
    for name, instruction, fn in groups():
        yield name, instruction, fn, 't2'
        if not aliases:
            continue
        mnemonic, operands = instruction.split(' ', 1)
        operands = operands.split(',')

        def spelling(rd, rs1, rs2=None):
            args = [rd, rs1] + operands[2:]
            if rs2 is not None:
                args[2] = rs2
            return mnemonic + ' ' + ','.join(args)

        yield name + '/rd-rs1', spelling('t0', 't0'), fn, 't0'
        yield name + '/rd-zero', spelling('zero', 't0'), lambda a, b: 0, 'zero'
        yield name + '/rs1-zero', spelling('t2', 'zero'), lambda a, b, fn=fn: fn(0, b), 't2'
        if operands[-1] == 't1':
            yield name + '/rd-rs2', spelling('t1', 't0'), fn, 't1'
            yield name + '/rs1-rs2', spelling('t2', 't0', 't0'), lambda a, b, fn=fn: fn(a, a), 't2'
            yield name + '/all-same', spelling('t0', 't0', 't0'), lambda a, b, fn=fn: fn(a, a), 't0'
            yield name + '/rs2-zero', spelling('t2', 't0', 'zero'), lambda a, b, fn=fn: fn(a, 0), 't2'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--oxy', default='oxyspike/target/release/oxyspike')
    parser.add_argument('--spike', default='build/spike-baseline/spike')
    parser.add_argument('--cc', default='riscv64-linux-gnu-gcc')
    parser.add_argument('--output', default='results/oxyspike/integer-validation.json')
    parser.add_argument('--aliases', action='store_true', help='also test overlapping registers and x0')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = (root / args.output).resolve()
    run_key = hashlib.sha256(str(output).encode()).hexdigest()[:16]
    work = root / 'build/oxyspike-tests/integer-vectors' / run_key
    work.mkdir(parents=True, exist_ok=True)
    edges = [0, 1, 2, 31, 32, 63, 64, 127, 128, 255, 65535,
             0x7fffffff, 0x80000000, 0xffffffff, 0x100000000,
             (1 << 63) - 1, 1 << 63, MASK - 1, MASK]
    rng = random.Random(0x0a715a1ce)
    pairs = [(a, b) for a in edges for b in edges]
    pairs += [(rng.getrandbits(64), rng.getrandbits(64)) for _ in range(128)]
    code = ['.option norvc', '.section .text', '.globl _start', '_start:']
    data = ['.section .rodata', '.balign 8']
    counts = {}
    for index, (name, instruction, fn, destination) in enumerate(register_layouts(args.aliases)):
        counts[name] = len(pairs)
        code += [f'la s0,vectors_{index}', f'li s1,{len(pairs)}', f'loop_{index}:',
                 'ld t0,0(s0)', 'ld t1,8(s0)', 'ld t3,16(s0)', instruction,
                 f'bne {destination},t3,fail', 'addi s0,s0,24', 'addi s1,s1,-1', f'bnez s1,loop_{index}']
        data.append(f'vectors_{index}:')
        data += [f'.dword {a:#x},{b:#x},{fn(a, b) & MASK:#x}' for a, b in pairs]
    code += ['li a0,1', 'j exit', 'fail:', 'li a0,3', 'exit:', 'la t0,tohost',
             'sd a0,0(t0)', '1: j 1b']
    data += ['.section .tohost,"aw",@progbits', '.balign 64', '.globl tohost',
             'tohost: .dword 0', '.balign 64', '.globl fromhost', 'fromhost: .dword 0']
    source = work / 'integer-vectors.S'
    source.write_text('\n'.join(code + data) + '\n')
    elf = work / 'integer-vectors.elf'
    isa = 'rv64imac_zba_zbb_zbs'
    commands = [[args.cc, '-nostdlib', '-static', '-march=' + isa, '-mabi=lp64',
                 '-Wl,-T,oxyspike/tests/link.ld', '-Wl,--build-id=none', str(source), '-o', str(elf)],
                [args.spike, '--isa=' + isa, str(elf)],
                [args.oxy, '--max-instructions', str(sum(counts.values()) * 16 + 10000), str(elf)]]
    logs = []
    for i, command in enumerate(commands):
        result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=120)
        log = work / f'integer-vectors-{i}.log'
        log.write_text(result.stdout + result.stderr)
        logs.append(log)
        if result.returncode:
            raise RuntimeError(f'{command[0]} failed ({result.returncode}); see {log}. Inspect s0 and s1 at fail to locate the vector.')
    files = [Path(__file__).resolve(), source, elf, root / args.oxy, root / args.spike, *logs]
    result = {'vectors': sum(counts.values()), 'groups': counts, 'seed': '0x0a715a1ce',
              'edge_pairs_per_group': len(edges) ** 2, 'random_pairs_per_group': 128, 'register_aliases': args.aliases,
              'scope': 'M/Zba/Zbb/Zbs arithmetic results: deterministic edges and random operands, all legal shift immediates. Python integer oracle checked by execution on both C++ Spike and OxySpike. Optional alias layouts are listed in group names. Not exhaustive encoding or arbitrary-register coverage.',
              'commands': commands, 'mismatches': 0,
              'sha256': {str(p.relative_to(root) if p.is_relative_to(root) else p): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(f"{result['vectors']} vectors in {len(counts)} groups passed on Spike and OxySpike.")


if __name__ == '__main__':
    main()
