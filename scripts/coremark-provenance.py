#!/usr/bin/env python3
"""Snapshot existing native binaries/profiles and host state before CoreMark runs."""
import hashlib
import json
from pathlib import Path
import subprocess
from datetime import datetime, timezone
ROOT=Path(__file__).resolve().parents[1]
def command(*args):
    return subprocess.check_output(args,text=True,cwd=ROOT).strip()
def main():
    result=ROOT/'results/coremark'
    result.mkdir(parents=True,exist_ok=True)
    if (result/'measurements.jsonl').exists():
        raise SystemExit('Archive the previous CoreMark results before taking a new provenance snapshot')
    paths=[f'build/{s}-{m}/{"spike" if s=="spike" else "qemu-system-riscv64"}'
           for s in ['spike','qemu'] for m in ['baseline','pgo']]
    paths += ['results/spike.profdata','results/qemu.profdata']
    hashes=[hashlib.sha256((ROOT/p).read_bytes()).hexdigest()+'  '+p for p in paths]
    host={'captured_at':datetime.now(timezone.utc).isoformat(),
          'cpu':command('sysctl','-n','machdep.cpu.brand_string'),
          'os':command('sw_vers'),
          'cpu_topology':command('sysctl','hw.ncpu','hw.perflevel0.physicalcpu','hw.perflevel1.physicalcpu'),
          'memory_bytes':command('sysctl','-n','hw.memsize'),
          'power_source':command('pmset','-g','batt'),
          'power_settings':command('pmset','-g','custom'),
          'native_execution':'Existing native simulator binaries; no new PGO collection'}
    (ROOT/'results/coremark-existing-binaries-profiles.sha256').write_text('\n'.join(hashes)+'\n')
    (result/'host.json').write_text(json.dumps(host,indent=2)+'\n')
    (result/'source-revision.txt').write_text(command('git','-C','coremark','rev-parse','HEAD')+'\n')
if __name__=='__main__': main()
