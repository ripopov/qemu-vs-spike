#!/usr/bin/env python3
"""Summarize xctrace's exported time-profile XML (including reference nodes)."""
import argparse
from collections import Counter
import gzip
import json
import xml.etree.ElementTree as ET

p = argparse.ArgumentParser()
p.add_argument('input')
p.add_argument('--after', type=float, default=0)
p.add_argument('--output', required=True)
a = p.parse_args()
opener = gzip.open if a.input.endswith('.gz') else open
with opener(a.input, 'rb') as f:
    root = ET.parse(f)
ids = {e.get('id'): e for e in root.iter() if e.get('id')}
def resolve(e):
    return ids[e.get('ref')] if e.get('ref') else e

functions, categories, cores, addresses = (Counter() for _ in range(4))
samples = 0
for row in root.findall('.//row'):
    if int(resolve(row.find('sample-time')).text) < a.after * 1e9:
        continue
    backtrace = row.find('backtrace')
    if backtrace is None:
        continue
    frames = [resolve(f) for f in resolve(backtrace)]
    if not frames:
        continue
    leaf = frames[0]
    name = leaf.get('name')
    weight = int(resolve(row.find('weight')).text)
    functions[name] += weight
    category = ('dispatch' if name.startswith('processor_t::step(') else
                'instruction handlers' if name.startswith('fast_rv') else 'other')
    categories[category] += weight
    cores[resolve(row.find('core')).get('fmt')] += weight
    binary = leaf.find('binary')
    if binary is not None and resolve(binary).get('name') == 'spike':
        binary = resolve(binary)
        # xctrace sample PCs may have low tagging bits set.
        offset = (int(leaf.get('addr'), 16) & ~3) - int(binary.get('load-addr'), 16)
        addresses[f'{name} + image offset {offset:#x}'] += weight
    samples += 1
total = sum(functions.values())
def entries(counter):
    return [dict(name=k,weight_ns=v,percent=100*v/total) for k,v in counter.most_common()]
result = dict(input=a.input,after_seconds=a.after,samples=samples,
              weight_ns=total,categories=entries(categories),cores=entries(cores),
              functions=entries(functions),addresses=entries(addresses))
with open(a.output,'w') as f:
    json.dump(result,f,indent=2)
    f.write('\n')
print(json.dumps({k:result[k] for k in ('samples','categories')},indent=2))
