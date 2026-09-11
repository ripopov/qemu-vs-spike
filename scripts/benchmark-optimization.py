#!/usr/bin/env python3
"""Compare non-PGO candidates using the existing guest and correctness checks."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import benchmark

spec = importlib.util.spec_from_file_location('coremark_bench', Path(__file__).with_name('benchmark-coremark.py'))
coremark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coremark)

p = argparse.ArgumentParser()
p.add_argument('--tag', required=True)
p.add_argument('--variants', nargs='+', default=['spike:baseline', 'spike:opt', 'qemu:baseline'])
p.add_argument('--workload', choices=['boot', 'coremark'], required=True)
p.add_argument('--runs', type=int, default=5)
p.add_argument('--warmups', type=int, default=1)
a = p.parse_args()
variants = [v.split(':') for v in a.variants]
if any(mode in ('pgo', 'train') for _, mode in variants):
    p.error('This experiment excludes PGO')
hashes = {(sim,mode): hashlib.sha256(Path(benchmark.command(sim,mode)[0]).read_bytes()).hexdigest()
          for sim,mode in variants}
workload = json.loads((benchmark.ROOT/'results/coremark/workload.json').read_text())
outpath = benchmark.ROOT/f'results/optimization/{a.tag}-{a.workload}.jsonl'
outpath.parent.mkdir(parents=True, exist_ok=True)
with outpath.open('x') as out:
    for i in range(-a.warmups, a.runs):
        for sim, mode in variants if i % 2 == 0 else reversed(variants):
            if a.workload == 'coremark':
                row = coremark.run(sim, mode, i, workload['iterations'], workload['seed'], False, a.tag)
            else:
                row = benchmark.run(sim, mode, i, tag=a.tag)
            row['warmup'] = i < 0
            row['binary_sha256'] = hashes[sim,mode]
            out.write(json.dumps(row)+'\n')
            out.flush()
            print(f'{sim}:{mode} {i}: boot={row["boot_seconds"]:.6f}s '
                  f'workload={row.get("workload_seconds", 0):.6f}s', flush=True)
