#!/usr/bin/env python3
"""Sum raw PMC deltas for the main thread of a CPU Counters trace after a start time."""
import subprocess, sys, xml.etree.ElementTree as ET
trace, after = sys.argv[1], float(sys.argv[2])
xmlp = trace.replace('.trace', '-kts.xml')
subprocess.run(['xcrun','xctrace','export','--input',trace,'--xpath',
  '/trace-toc/run[@number="1"]/data/table[@schema="kdebug-counters-with-time-sample"]',
  '--output',xmlp],check=True,capture_output=True)
root=ET.parse(xmlp); ids={e.get('id'):e for e in root.iter() if e.get('id')}
res=lambda e: ids[e.get('ref')] if e.get('ref') is not None else e
prev={}; tot=None; n=0; t0=None; t1=None
for row in root.findall('.//row'):
    k=list(row); t=int(res(k[0]).text); th=res(k[1]).get('fmt'); core=res(k[2]); pe=res(k[7])
    if pe.tag!='pmc-events' or core.tag=='sentinel' or (len(sys.argv)>3 and sys.argv[3] not in th): continue
    vals=[int(x) for x in pe.text.split()]; key=core.get('fmt')
    if key in prev and t>after*1e9:
        pt,pv=prev[key]
        if t-pt<5e6:
            d=[a-b for a,b in zip(vals,pv)]
            if all(x>=0 for x in d):
                tot=d if tot is None else [a+b for a,b in zip(tot,d)]; n+=1
                t0=pt if t0 is None else t0; t1=t
    prev[key]=(t,vals)
print('intervals',n,'span_s',(t1-t0)/1e9 if t0 else None)
print('sums',tot)
print('GHz',tot[0]/((t1-t0)/1e9)/1e9)
print('per-cycle',[round(x/tot[0],4) for x in tot])
