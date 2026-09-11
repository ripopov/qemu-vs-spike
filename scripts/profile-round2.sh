#!/bin/bash
# Time Profiler recordings of a Spike variant: 8 s attached inside the CoreMark
# loop, and five complete Linux boots aggregated.  Summaries go to results/round2.
set -euo pipefail
cd "$(dirname "$0")/.."
variant=${1:?usage: profile-round2.sh <variant>}
R=$PWD/build/round2; OUT=$PWD/results/round2; mkdir -p "$R" "$OUT"
ISA=rv64imafdc_zicsr_zifencei_zihintpause_zba_zbb_zbs_zfhmin_zkt_zicntr_zihpm_zicbom_zicbop_zicboz_svpbmt_svinval_svade
spike=build/spike-$variant/spike
rm -rf "$R/$variant-coremark.trace"
$spike -p1 -m256 --isa=$ISA --dtb=results/platform.dtb guest/coremark/fw_payload.elf > "$R/$variant-coremark-guest.log" 2>&1 & pid=$!
until grep -aq BENCH_COREMARK_START "$R/$variant-coremark-guest.log"; do sleep 0.05; done
xcrun xctrace record --template 'Time Profiler' --time-limit 8s --output "$R/$variant-coremark.trace" --attach $pid > /dev/null 2>&1
wait $pid
xcrun xctrace export --input "$R/$variant-coremark.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]' --output "$R/$variant-coremark.xml" > /dev/null 2>&1
gzip -kf "$R/$variant-coremark.xml" && mv "$R/$variant-coremark.xml.gz" "$OUT/"
python3 scripts/summarize-spike-profile.py "$R/$variant-coremark.xml" --after 0 --output "$OUT/$variant-coremark-hotspots.json" > /dev/null
for i in 1 2 3 4 5; do
  rm -rf "$R/$variant-boot$i.trace"
  xcrun xctrace record --template 'Time Profiler' --output "$R/$variant-boot$i.trace" --target-stdout /dev/null --launch -- $spike -p1 -m256 --isa=$ISA --dtb=results/platform.dtb guest/fw_payload.elf > /dev/null 2>&1
  xcrun xctrace export --input "$R/$variant-boot$i.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]' --output "$R/$variant-boot$i.xml" > /dev/null 2>&1
  python3 scripts/summarize-spike-profile.py "$R/$variant-boot$i.xml" --after 0 --output "$R/$variant-boot$i-hotspots.json" > /dev/null
done
cat "$R/$variant-boot"[1-5].xml | gzip > "$OUT/$variant-boot.xml.gz"
python3 - "$variant" <<'PY'
import json, collections, sys
v=sys.argv[1]; tot=collections.Counter(); cats=collections.Counter(); n=0; w=0
for i in range(1,6):
    p=json.load(open(f'build/round2/{v}-boot{i}-hotspots.json')); n+=p['samples']; w+=p['weight_ns']
    for r in p['functions']: tot[r['name']]+=r['weight_ns']
    for c in p['categories']: cats[c['name']]+=c['weight_ns']
T=sum(tot.values())
out=dict(input=f'five launches of build/spike-{v}/spike on guest/fw_payload.elf',samples=n,weight_ns=T,
         categories=[dict(name=k,weight_ns=x,percent=100*x/T) for k,x in cats.most_common()],
         functions=[dict(name=k,weight_ns=x,percent=100*x/T) for k,x in tot.most_common()])
json.dump(out,open(f'results/round2/{v}-boot-hotspots.json','w'),indent=2)
print('boot samples',n,{c['name']:round(c['percent'],1) for c in out['categories']})
for r in out['functions'][:12]: print(f"{r['percent']:6.2f} {r['name'][:90]}")
PY
python3 - "$variant" <<'PY'
import json,sys; v=sys.argv[1]; p=json.load(open(f'results/round2/{v}-coremark-hotspots.json'))
print('coremark samples',p['samples'],{c['name']:round(c['percent'],1) for c in p['categories']})
for r in p['functions'][:12]: print(f"{r['percent']:6.2f} {r['name'][:90]}")
PY
