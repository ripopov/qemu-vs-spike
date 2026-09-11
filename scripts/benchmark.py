#!/usr/bin/env python3
"""Record wall time to a BusyBox shell marker, and full boot/shutdown time."""
import argparse, json, os, re, selectors, subprocess, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
ISA = 'rv64imafdc_zicsr_zifencei_zihintpause_zba_zbb_zbs_zfhmin_zkt_zicntr_zihpm_zicbom_zicbop_zicboz_svpbmt_svinval_svade'
MARKER = b'BENCH_BUSYBOX_READY'
def command(sim, mode, counted=False):
    if sim == 'oxyspike':
        if not mode or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789-_' for c in mode):
            raise ValueError('Invalid OxySpike variant name')
        binary = ROOT/'oxyspike/target/release/oxyspike' if mode == 'release' else ROOT/f'build/oxyspike-variants/{mode}'
        if counted:
            raise ValueError('OxySpike histogram counting is not implemented')
        return [str(binary), '--dtb',
                str(ROOT/'results/platform.dtb'), str(ROOT/'guest/fw_payload.elf')]
    if sim == 'spike':
        args = [str(ROOT / f'build/spike-{mode}/spike'), '-p1', '-m256', '--isa='+ISA, '--dtb='+str(ROOT/'results/platform.dtb')]
        if counted: args += ['-g']
        return args + [str(ROOT/'guest/fw_payload.elf')]
    args = [str(ROOT/f'build/qemu-{mode}/qemu-system-riscv64'), '-M', 'spike', '-cpu', 'rva22s64', '-smp', '1', '-m', '256M', '-accel', 'tcg,thread=single', '-nographic', '-monitor', 'none', '-bios', str(ROOT/'guest/fw_payload.elf')]
    if counted: args += ['-plugin', str(ROOT/'build/libinsn.dylib'), '-d', 'plugin']
    return args

def run(sim, mode, index, counted=False, timeout=180, tag="run"):
    cmd = command(sim, mode, counted)
    env = os.environ.copy()
    if mode == "train":
        profile_dir = ROOT/f"results/profiles/{sim}"
        profile_dir.mkdir(parents=True, exist_ok=True)
        env["LLVM_PROFILE_FILE"] = str(profile_dir/"%m-%p.profraw")
    start = time.perf_counter_ns()
    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=ROOT, env=env)
    sel = selectors.DefaultSelector(); sel.register(proc.stdout, selectors.EVENT_READ)
    data = bytearray(); ready = None
    try:
        while sel.get_map():
            if (time.perf_counter_ns()-start)/1e9 > timeout:
                raise TimeoutError(f'{sim}/{mode} boot timed out')
            for key, _ in sel.select(.1):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk: sel.unregister(key.fileobj); continue
                data.extend(chunk)
                if ready is None and MARKER in data:
                    ready = (time.perf_counter_ns()-start)/1e9
        rc = proc.wait(timeout=5)
        elapsed = (time.perf_counter_ns()-start)/1e9
    finally:
        if proc.poll() is None: proc.kill(); proc.wait()
        sel.close()
        log = ROOT/f'results/{tag}-{sim}-{mode}-{"count" if counted else "time"}-{index:02d}.log'
        log.write_bytes(data)
    if ready is None or rc != 0:
        raise RuntimeError(f'{sim}/{mode}: rc={rc}, marker={ready}, see {log}')
    insns = None
    if counted:
        if sim == 'qemu':
            match = re.search(rb'total insns: (\d+)', data)
            if match: insns = int(match[1])
        else:
            lines = data.split(b'PC Histogram size:')[-1].splitlines()[1:]
            insns = sum(int(m[1]) for line in lines if (m := re.fullmatch(rb'[0-9a-f]+ (\d+)', line)))
        if not insns: raise RuntimeError('No instruction count')
    return dict(simulator=sim, mode=mode, index=index, counted=counted, boot_seconds=ready, process_seconds=elapsed, instructions=insns, command=cmd, log=str(log.relative_to(ROOT)), timestamp=time.time())

def main():
    p=argparse.ArgumentParser(); p.add_argument('--modes',nargs='+',default=['baseline','pgo']); p.add_argument('--simulators',nargs='+',default=['spike','qemu']); p.add_argument('--runs',type=int,default=10); p.add_argument('--warmups',type=int,default=2); p.add_argument('--counted',action='store_true'); p.add_argument('--output',default='results/measurements.jsonl'); a=p.parse_args()
    with (ROOT/a.output).open('a') as out:
        for i in range(-a.warmups,a.runs):
            combos=[(s,m) for m in a.modes for s in a.simulators]
            if i%2: combos.reverse()
            for sim,mode in combos:
                row=run(sim,mode,i,a.counted,tag=Path(a.output).stem); row['warmup']=i<0
                out.write(json.dumps(row)+'\n'); out.flush()
                print(f'{sim:5} {mode:8} {i:3}: boot {row["boot_seconds"]:.6f}s total {row["process_seconds"]:.6f}s instructions={row["instructions"]}',flush=True)
if __name__=='__main__': main()
