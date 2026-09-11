#!/usr/bin/env python3
"""Record host, compiler, source, and artifact identities for the second round."""
import hashlib, json, platform, subprocess, datetime
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
def sh(*a): return subprocess.run(a, capture_output=True, text=True, cwd=ROOT).stdout.strip()
def digest(p): return hashlib.sha256((ROOT/p).read_bytes()).hexdigest()
files = ['build/spike-baseline/spike','build/spike-opt/spike','build/spike-opt2/spike','build/qemu-baseline/qemu-system-riscv64',
         'guest/fw_payload.elf','guest/coremark/fw_payload.elf','results/platform.dtb',
         'results/round2/fetch-regime.patch','results/round2/fetch-regime-exact-sfence-16k.patch','results/round2/fetch-regime-exact-sfence-64k.patch',
         'results/round2/cold-slow-path.patch','results/round2/round2.patch','results/round2/fetch-regime-sfence.patch']
prov = dict(date=datetime.datetime.now().astimezone().isoformat(), host=platform.platform(), cpu=sh('sysctl','-n','machdep.cpu.brand_string'),
            macos=sh('sw_vers','-productVersion'), compiler=sh('/opt/homebrew/opt/llvm@20/bin/clang++','--version').splitlines()[0],
            flags='-O3 -flto=thin', pgo=False,
            spike_commit=sh('git','-C','spike','rev-parse','HEAD'), spike_branch=sh('git','-C','spike','rev-parse','--abbrev-ref','HEAD'),
            spike_worktree_diff_sha256=hashlib.sha256(sh('git','-C','spike','diff').encode()).hexdigest(),
            qemu_commit=sh('git','-C','qemu','rev-parse','HEAD'), coremark_commit=sh('git','-C','coremark','rev-parse','HEAD'),
            parent_commit=sh('git','rev-parse','HEAD'),
            icache_entries=65536, sha256={f:digest(f) for f in files})
(ROOT/'results/round2/provenance.json').write_text(json.dumps(prov, indent=2)+'\n')
# Preserved original artifacts must be unchanged.
lines=[]
for line in (ROOT/'results/coremark-existing-binaries-profiles.sha256').read_text().splitlines():
    expected,path=line.split(None,1); ok=digest(path.strip())==expected; lines.append(f'{"OK" if ok else "CHANGED"} {path.strip()} {expected}')
    assert ok, path
for path,expected in [('build/spike-opt/spike','aae1a388cfd80a542928f309a8ac7d6a79e784bbb2794ea8e5b58cb58eb5c9f3'),('guest/fw_payload.elf','1c6e4c24be174bae3b2871d3f4b078d87ad43ff9837ecf48ec4634bbf090e77a'),('guest/coremark/fw_payload.elf','3b093a152ebb388ceee29ae5e8d8d9f07559027c3a717fc574cd444296b02459'),('results/platform.dtb','5315aa3bbfc200cc2e8064e4287e02cdcdc1551334a978668b9b12c595a16646')]:
    ok=digest(path)==expected; lines.append(f'{"OK" if ok else "CHANGED"} {path} {expected}'); assert ok, path
(ROOT/'results/round2/unchanged-artifacts-check.txt').write_text('\n'.join(lines)+'\n')
print(json.dumps({k:v for k,v in prov.items() if k!='sha256'}, indent=2))
