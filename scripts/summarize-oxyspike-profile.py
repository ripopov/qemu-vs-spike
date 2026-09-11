#!/usr/bin/env python3
"""Attribute process-local PC samples to nearest ELF text symbols."""
import argparse,bisect,collections,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('binary');p.add_argument('profile');a=p.parse_args()
text=subprocess.check_output(['nm','-n','-C',a.binary],text=True)
symbols=[]
for line in text.splitlines():
    parts=line.split(maxsplit=2)
    if len(parts)==3 and parts[1] in ('t','T'):
        symbols.append((int(parts[0],16),parts[2]))
addresses=[s[0] for s in symbols];counts=collections.Counter();buckets=[];total=0
for line in Path(a.profile).read_text().splitlines():
    if line.startswith('#'):continue
    pc,n=line.split();pc=int(pc,16);n=int(n);total+=n
    buckets.append((n,pc))
    i=bisect.bisect_right(addresses,pc)-1
    counts[symbols[i][1] if i>=0 else 'unknown']+=n
print(json.dumps({'samples':total,'method':'glibc profil, 64-byte PC buckets; nearest-symbol attribution',
    'buckets':[{'address':hex(pc),'samples':n} for n,pc in sorted(buckets,reverse=True)[:20]],
    'hotspots':[{'symbol':s,'samples':n,'percent':round(100*n/total,2)} for s,n in counts.most_common(15)]},indent=2))
if total==0:raise SystemExit('No samples collected')
