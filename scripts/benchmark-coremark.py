#!/usr/bin/env python3
"""Host-clock CoreMark workload comparison using existing simulator binaries."""
import argparse
import json
import os
from pathlib import Path
import re
import selectors
import subprocess
import time
from benchmark import ROOT, command

START = b'BENCH_COREMARK_START'
END = b'BENCH_COREMARK_END'

def run(sim, mode, index, iterations, seed, counted, tag, timeout=300):
    cmd = command(sim, mode)
    old = str(ROOT/'guest/fw_payload.elf')
    cmd = [str(ROOT/'guest/coremark/fw_payload.elf') if a == old else a for a in cmd]
    if counted and sim == 'qemu':
        cmd += ['-plugin', str(ROOT/'build/libcoremark-count.dylib'), '-d', 'plugin']
    data = bytearray()
    stamps = {}
    launch = time.perf_counter_ns()
    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, cwd=ROOT)
    sel = selectors.DefaultSelector()
    sel.register(proc.stdout, selectors.EVENT_READ)
    log = ROOT/f'results/coremark/{tag}-{sim}-{mode}-{index:02d}.log'
    try:
        while sel.get_map():
            if (time.perf_counter_ns()-launch)/1e9 > timeout:
                raise TimeoutError(f'{sim}/{mode}: see {log}')
            for key, _ in sel.select(.1):
                chunk = os.read(key.fileobj.fileno(), 65536)
                now = time.perf_counter_ns()
                if not chunk:
                    sel.unregister(key.fileobj)
                    continue
                data.extend(chunk)
                for marker in (b'BENCH_BUSYBOX_READY', START, END):
                    if marker not in stamps and marker in data:
                        stamps[marker] = now
        rc = proc.wait(timeout=5)
        finished = time.perf_counter_ns()
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        sel.close()
        proc.stdout.close()
        log.write_bytes(data)
    if rc != 0 or b'Power down' not in data or b'BENCH_COREMARK_EXIT=0' not in data or START not in stamps or END not in stamps:
        raise RuntimeError(f'{sim}/{mode} failed: rc={rc}, see {log}')
    if data.count(START) != 1 or data.count(END) != 1:
        raise RuntimeError('Expected exactly one fixed-iteration workload interval')
    crcs = {}
    for name in ('seedcrc','crclist','crcmatrix','crcstate','crcfinal'):
        m = re.search(name.encode()+rb'\s*:\s*(0x[0-9a-f]+)', data)
        if not m:
            raise RuntimeError(f'Missing {name}: {log}')
        crcs[name] = int(m[1],16)
    expected = ((0xe9f5,0xe714,0x1fd7,0x8e3a) if seed == '0' else
                (0x18f2,0xe3c1,0x0747,0x8d84))
    if tuple(crcs[k] for k in ('seedcrc','crclist','crcmatrix','crcstate')) != expected:
        raise RuntimeError(f'CoreMark CRC mismatch: {crcs}, see {log}')
    errors = re.findall(rb'[^\r\n]*ERROR![^\r\n]*', data)
    if b'Cannot validate operation' in data or any(b'Must execute for at least 10 secs' not in e for e in errors):
        raise RuntimeError(f'CoreMark error: {errors}, see {log}')
    actual_iterations = re.search(rb'Iterations\s*:\s*(\d+)',data)
    if not actual_iterations or int(actual_iterations[1]) != iterations:
        raise RuntimeError('Iteration count mismatch')
    instret = re.search(rb'BENCH_INSTRET_DELTA=(\d+)', data)
    insns = int(instret[1]) if sim in ('spike', 'oxyspike') and instret else None
    if counted and sim == 'qemu':
        m = re.search(rb'BENCH_QEMU_ROI_INSNS=(\d+) starts=1 ends=1',data)
        if not m: raise RuntimeError(f'Missing QEMU workload count: {log}')
        insns = int(m[1])
    seconds = (stamps[END]-stamps[START])/1e9
    if seconds <= 0 or (insns is not None and insns <= 0):
        raise RuntimeError('Nonpositive workload time or instruction count')
    return dict(simulator=sim,mode=mode,index=index,iterations=iterations,seed=seed,
                counted=counted,workload_seconds=seconds,
                boot_seconds=(stamps[b'BENCH_BUSYBOX_READY']-launch)/1e9,
                process_seconds=(finished-launch)/1e9,instructions=insns,crcs=crcs,
                guest_duration_warning=bool(errors),command=cmd,
                log=str(log.relative_to(ROOT)),timestamp=time.time())

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--iterations',type=int,required=True)
    p.add_argument('--seed',choices=['0','0x3415'],default='0')
    p.add_argument('--modes',nargs='+',choices=['baseline','pgo'],default=['baseline','pgo'])
    p.add_argument('--simulators',nargs='+',choices=['spike','qemu'],default=['spike','qemu'])
    p.add_argument('--runs',type=int,default=10)
    p.add_argument('--warmups',type=int,default=2)
    p.add_argument('--counted',action='store_true')
    p.add_argument('--output',default='results/coremark/measurements.jsonl')
    a=p.parse_args()
    if a.iterations <= 0: p.error('iterations must be positive; auto-calibration in the guest is forbidden')
    workload=json.loads((ROOT/"results/coremark/workload.json").read_text())
    if workload != {"iterations": a.iterations, "seed": a.seed}:
        p.error("Requested parameters do not match the built guest workload.json")
    outpath=ROOT/a.output
    outpath.parent.mkdir(parents=True,exist_ok=True)
    with outpath.open('x') as out:
        for i in range(-a.warmups,a.runs):
            pairs=[(s,m) for m in a.modes for s in a.simulators]
            if i%2: pairs.reverse()
            for sim,mode in pairs:
                row=run(sim,mode,i,a.iterations,a.seed,a.counted,outpath.stem)
                row['warmup']=i<0
                out.write(json.dumps(row)+'\n'); out.flush()
                print(f'{sim:5} {mode:8} {i:3}: workload {row["workload_seconds"]:.6f}s boot {row["boot_seconds"]:.6f}s instructions={row["instructions"]} CRC=OK',flush=True)
if __name__=='__main__': main()
