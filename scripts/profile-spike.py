#!/usr/bin/env python3
"""Record and export a non-PGO Spike Time Profiler trace on macOS."""
import argparse
import gzip
from pathlib import Path
import subprocess
from benchmark import ROOT, command

p = argparse.ArgumentParser()
p.add_argument('--mode', choices=['baseline','opt'], required=True)
p.add_argument('--workload', choices=['boot','coremark'], required=True)
p.add_argument('--name', required=True)
p.add_argument('--seconds', type=int, default=60)
a = p.parse_args()
trace = ROOT/f'build/optimization/{a.name}.trace'
xml = trace.with_suffix('.xml')
logs = ROOT/'results/optimization'
logs.mkdir(parents=True,exist_ok=True)
trace.parent.mkdir(parents=True,exist_ok=True)
cmd = command('spike',a.mode)
if a.workload == 'coremark':
    cmd[-1] = str(ROOT/'guest/coremark/fw_payload.elf')
record = ['xcrun','xctrace','record','--template','Time Profiler',
          '--time-limit',f'{a.seconds}s','--output',str(trace),
          '--target-stdout',str(logs/f'{a.name}-guest.log'),'--launch','--',*cmd]
result = subprocess.run(record, cwd=ROOT)
if result.returncode not in (0,54):
    raise SystemExit(result.returncode)
subprocess.run(['xcrun','xctrace','export','--input',str(trace),'--toc',
                '--output',str(logs/f'{a.name}-toc.xml')],check=True)
subprocess.run(['xcrun','xctrace','export','--input',str(trace),
                '--xpath','/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]',
                '--output',str(xml)],check=True)
packed = logs/f'{a.name}.xml.gz'
with gzip.open(packed,'wb') as out:
    out.write(xml.read_bytes())
subprocess.run(['python3',str(ROOT/'scripts/summarize-spike-profile.py'),str(packed),
                '--after','2' if a.workload == 'coremark' else '0',
                '--output',str(logs/f'{a.name}-hotspots.json')],check=True)
