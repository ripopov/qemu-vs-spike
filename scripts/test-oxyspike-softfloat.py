#!/usr/bin/env python3
"""Compare Rust arithmetic bits AND IEEE flags against Spike's SoftFloat library."""
import json, random, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1]
rng=random.Random(0x0a71591)
cases=[]
for fmt in (32,64):
    frac,exp=(23,8) if fmt==32 else (52,11)
    sign=1<<(fmt-1); inf=((1<<exp)-1)<<frac; one=((1<<(exp-1))-1)<<frac
    values=[0,1,(1<<frac)-1,1<<frac,one-1,one,one+1,one+(1<<(frac-1)),inf-1,inf,inf+1,inf+(1<<(frac-1)),((1<<(exp-1))-2)<<frac]
    values += [v|sign for v in values]
    for rm in range(5):
        for a in values:
            for b in values:
                for op in range(5): cases.append((fmt,op,rm,a,b,rng.choice(values)))
            for op in range(5,14):cases.append((fmt,op,rm,a,0,0))
        for _ in range(2000):
            cases.append((fmt,rng.randrange(14),rm,rng.getrandbits(fmt),rng.getrandbits(fmt),rng.getrandbits(fmt)))
data=''.join(f'{f} {o} {r} {a:x} {b:x} {c:x}\n' for f,o,r,a,b,c in cases)
ref=subprocess.run([str(root/'build/oxyspike-tests/softfloat-reference')],input=data,text=True,capture_output=True,check=True).stdout.splitlines()
actual=subprocess.run([str(root/'oxyspike/target/release/examples/softfloat-check')],input=data,text=True,capture_output=True,check=True).stdout.splitlines()
assert len(ref)==len(actual)==len(cases)
errors=[dict(case=c,reference=r,actual=a) for c,r,a in zip(cases,ref,actual) if r!=a]
from collections import Counter
print(json.dumps(dict(cases=len(cases),mismatches=len(errors),examples=errors[:12],
    by_pair=dict(Counter(f'{e["reference"].split()[1]} -> {e["actual"].split()[1]}' for e in errors))),indent=2))
if errors: raise SystemExit(1)
# Reuse oracle answers in a guest fixture that checks the instruction decoder,
# register routing, NaN boxing, rounding-mode fields, and accrued fflags.
import struct
binary=bytearray()
for (fmt,op,rm,a,b,c),result in zip(cases,ref):
    expected,flags=(int(result.split()[0],16),int(result.split()[1]))
    single=fmt==32
    f=0 if single else 1
    rs1,rs2,rd=1,2,28
    if op==4:
        insn=(3<<27)|(f<<25)|(rs2<<20)|(rs1<<15)|(rm<<12)|(rd<<7)|0x43
    else:
        base={0:0,1:8,2:12,3:0x2c,5:0x20,6:0x60,7:0x60,8:0x60,9:0x60,10:0x68,11:0x68,12:0x68,13:0x68}[op]
        if op==3:rs2=0
        if op==5:rs2=f;f^=1
        if 6<=op<=9:rs2=op-6
        if op>=10:rs2=op-10;rs1=10
        insn=((base|f)<<25)|(rs2<<20)|(rs1<<15)|(rm<<12)|(rd<<7)|0x53
    if single and op<10:a|=0xffffffff00000000
    if single:b|=0xffffffff00000000;c|=0xffffffff00000000
    result_kind=1 if 6<=op<=9 else 2 if f==0 else 0
    binary.extend(struct.pack('<5QIB3x',a,b,c,expected,flags,insn,result_kind))
(root/'build/oxyspike-tests/floating-cases.bin').write_bytes(binary)
