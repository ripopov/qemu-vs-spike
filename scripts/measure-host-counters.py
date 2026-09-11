#!/usr/bin/env python3
"""Sample Linux perf counters inside CoreMark, after its initial warmup.

Host/guest ratios estimate window guest instructions from full-loop throughput.
IPC uses counters from the same window. Outputs are local, ignored artifacts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import selectors
import signal
import statistics
import subprocess
import time

from benchmark import ROOT, command

EVENTS = '{cpu_core/instructions/,cpu_core/cpu-cycles/}'
START = b'BENCH_COREMARK_START'
END = b'BENCH_COREMARK_END'


def control(fd, ack, message):
    before = time.perf_counter()
    os.write(fd, message.encode() + b'\n')
    if not select.select([ack], [], [], 2)[0] or os.read(ack, 64).rstrip(b'\x00') != b'ack\n':
        raise RuntimeError(f'perf did not acknowledge {message}')
    after = time.perf_counter()
    return (before + after) / 2, after - before


def measure(sim, mode, index, args, directory):
    guest = command(sim, mode)
    guest = [str(ROOT / 'guest/coremark/fw_payload.elf')
             if part == str(ROOT / 'guest/fw_payload.elf') else part for part in guest]
    prefix = directory / f'{sim}-{mode}-{index}'
    counters = prefix.with_suffix('.perf.json')
    ctl_read, ctl_write = os.pipe()
    ack_read, ack_write = os.pipe()
    perf = ['perf', 'stat', '-D', '-1', '--control', f'fd:{ctl_read},{ack_write}',
            '-j', '-o', str(counters), '-e', EVENTS, '--',
            'taskset', '-c', str(args.cpu), *guest]
    data = bytearray()
    stamps = {}
    enabled = disabled = None
    latencies = []
    launched = time.perf_counter()
    env = dict(os.environ, LC_ALL='C')
    try:
        with prefix.with_suffix('.perf.log').open('wb') as errors:
            proc = subprocess.Popen(perf, cwd=ROOT, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.PIPE, stderr=errors,
                                    pass_fds=(ctl_read, ack_write), start_new_session=True,
                                    env=env)
            os.close(ctl_read)
            os.close(ack_write)
            ctl_read = ack_write = None
            try:
                with selectors.DefaultSelector() as selector:
                    selector.register(proc.stdout, selectors.EVENT_READ)
                    while selector.get_map():
                        now = time.perf_counter()
                        if now - launched > 180:
                            raise TimeoutError('CoreMark timed out')
                        if START in stamps and END not in stamps:
                            if enabled is None and now >= stamps[START] + args.settle:
                                enabled, latency = control(ctl_write, ack_read, 'enable')
                                latencies.append(latency)
                            elif enabled is not None and disabled is None and now >= enabled + args.window:
                                disabled, latency = control(ctl_write, ack_read, 'disable')
                                latencies.append(latency)
                        for key, _ in selector.select(0.01):
                            chunk = os.read(key.fileobj.fileno(), 65536)
                            if not chunk:
                                selector.unregister(key.fileobj)
                                continue
                            data.extend(chunk)
                            for marker in (START, END):
                                if marker not in stamps and marker in data:
                                    stamps[marker] = time.perf_counter()
                rc = proc.wait(timeout=5)
            finally:
                if proc.poll() is None:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                proc.stdout.close()
    finally:
        for fd in (ctl_read, ctl_write, ack_read, ack_write):
            if fd is not None:
                os.close(fd)
        prefix.with_suffix('.guest.log').write_bytes(data)
    if rc or b'BENCH_COREMARK_EXIT=0' not in data or b'Power down' not in data:
        raise RuntimeError(f'Guest/perf failed; inspect {prefix}')
    if data.count(START) != 1 or data.count(END) != 1:
        raise RuntimeError('Expected exactly one CoreMark interval')
    if enabled is None or disabled is None or not stamps[START] < enabled < disabled < stamps[END]:
        raise RuntimeError('Counter window was not entirely inside CoreMark')
    expected = dict(seedcrc=0xe9f5, crclist=0xe714, crcmatrix=0x1fd7,
                    crcstate=0x8e3a, crcfinal=0x988c)
    for name, value in expected.items():
        match = re.search(name.encode() + rb'\s*:\s*(0x[0-9a-f]+)', data)
        if not match or int(match[1], 16) != value:
            raise RuntimeError(f'{name} mismatch')
    iterations = re.search(rb'Iterations\s*:\s*(\d+)', data)
    if not iterations or int(iterations[1]) != 32000:
        raise RuntimeError('Expected 32,000 iterations')
    errors = re.findall(rb'[^\r\n]*ERROR![^\r\n]*', data)
    if b'Cannot validate operation' in data or any(b'Must execute for at least 10 secs' not in e for e in errors):
        raise RuntimeError('Guest validation error')
    if sim == 'qemu':
        guest_instructions = args.qemu_instructions
        if not guest_instructions:
            raise ValueError('QEMU requires --qemu-instructions from a separate counted run')
    else:
        match = re.search(rb'BENCH_INSTRET_DELTA=(\d+)', data)
        if not match:
            raise RuntimeError('Missing guest instruction count')
        guest_instructions = int(match[1])
    events = {}
    for line in counters.read_text().splitlines():
        if not line.strip().startswith('{'):
            continue
        event = json.loads(line.rstrip(','))
        if float(event['pcnt-running']) < 99:
            raise RuntimeError(f'Counter multiplexing: {event}')
        events[event['event']] = float(event['counter-value'])
    instructions = next(v for k, v in events.items() if 'instructions' in k)
    cycles = next(v for k, v in events.items() if 'cpu-cycles' in k)
    seconds = stamps[END] - stamps[START]
    window = disabled - enabled
    estimated_guest = guest_instructions * window / seconds
    return dict(simulator=sim, mode=mode, index=index, cpu=args.cpu,
                events=EVENTS, host_instructions=instructions, host_cycles=cycles,
                host_ipc=instructions / cycles,
                host_instructions_per_guest=instructions / estimated_guest,
                host_cycles_per_guest=cycles / estimated_guest,
                guest_instructions=guest_instructions, estimated_window_guest_instructions=estimated_guest,
                coremark_seconds=seconds, window_seconds=window,
                window_start_after_marker=enabled - stamps[START], control_latencies=latencies,
                crcs=expected, command=perf,
                binary_sha256=hashlib.sha256(Path(guest[0]).read_bytes()).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--variants', nargs='+', default=['spike:opt3', 'qemu:baseline',
                                                        'oxyspike:release', 'oxyspike:refactor-pgo'])
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--cpu', type=int, default=0)
    parser.add_argument('--settle', type=float, default=0.75)
    parser.add_argument('--window', type=float, default=1.5)
    parser.add_argument('--qemu-instructions', type=int)
    args = parser.parse_args()
    if not re.fullmatch(r'[a-zA-Z0-9_-]+', args.tag):
        parser.error('Use a simple unique output tag')
    if args.runs < 1 or args.settle <= 0 or args.window <= 0:
        parser.error('Runs, settle and window must be positive')
    # Fail before launching a guest if permissions or PMU availability block perf.
    subprocess.run(['perf', 'stat', '-e', EVENTS, '--', 'taskset', '-c', str(args.cpu), 'true'], check=True)
    directory = ROOT / 'results' / 'host-counters' / args.tag
    directory.mkdir(parents=True, exist_ok=False)
    variants = [item.split(':') for item in args.variants]
    rows = []
    with (directory / 'measurements.jsonl').open('x') as output:
        for index in range(args.runs):
            for sim, mode in variants if index % 2 == 0 else reversed(variants):
                row = measure(sim, mode, index, args, directory)
                rows.append(row)
                output.write(json.dumps(row) + '\n')
                output.flush()
                print(f'{sim}:{mode} {index}: host/guest={row["host_instructions_per_guest"]:.2f}, IPC={row["host_ipc"]:.2f}', flush=True)
    summary = {}
    for sim, mode in variants:
        group = [r for r in rows if r['simulator'] == sim and r['mode'] == mode]
        summary[f'{sim}:{mode}'] = {key: statistics.median(r[key] for r in group)
                                   for key in ('host_ipc', 'host_instructions_per_guest', 'host_cycles_per_guest')}
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
