#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
build/qemu-baseline/qemu-system-riscv64 -M spike,dumpdtb=results/qemu-platform.dtb -cpu rva22s64 -smp 1 -m 256M -nographic -bios guest/fw_payload.elf
dtc -I dtb -O dts results/qemu-platform.dtb -o results/qemu-platform.dts
python3 - <<'PY'
import re, sys
from pathlib import Path
sys.path.insert(0, 'scripts')
from benchmark import ISA
s=Path('results/qemu-platform.dts').read_text()
s=re.sub(r'riscv,isa = "[^"]*";', 'riscv,isa = "'+ISA+'";', s)
Path('results/platform.dts').write_text(s)
PY
dtc -I dts -O dtb results/platform.dts -o results/platform.dtb
/opt/homebrew/opt/llvm@20/bin/clang -O3 -shared -fPIC -undefined dynamic_lookup $(pkg-config --cflags glib-2.0) -I qemu/include/plugins scripts/insn-count.c -o build/libinsn.dylib $(pkg-config --libs glib-2.0)
