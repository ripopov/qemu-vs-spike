#!/usr/bin/env python3
"""Print per-variant medians for an optimization JSONL series."""
import json, statistics as st, sys
for path in sys.argv[1:]:
    rows=[json.loads(l) for l in open(path) if l.strip()]
    key='workload_seconds' if 'workload_seconds' in rows[0] else 'boot_seconds'
    print(path)
    for k in sorted({(r['simulator'],r['mode']) for r in rows}):
        v=[r[key] for r in rows if (r['simulator'],r['mode'])==k and not r['warmup']]
        extra=''
        if key=='workload_seconds' and k[0]=='spike':
            extra=f" MIPS={st.median(r['instructions']/r['workload_seconds']/1e6 for r in rows if (r['simulator'],r['mode'])==k and not r['warmup']):.2f} instret={ {r['instructions'] for r in rows if (r['simulator'],r['mode'])==k} }"
        print(f"  {k[0]}:{k[1]:10} n={len(v):2} median={st.median(v):.6f} mean={st.mean(v):.6f} min={min(v):.6f} max={max(v):.6f}{extra}")
