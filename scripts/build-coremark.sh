#!/bin/bash
set -euo pipefail
mkdir -p /build/guest/rootfs/bin /work/results/coremark
cd /work/coremark
riscv64-linux-gnu-gcc -O3 -static -march=rv64gc_zba_zbb_zbs_zfhmin -mabi=lp64d \
 -I. -Ilinux -Iposix '-DFLAGS_STR="-O3 -static -march=rv64gc_zba_zbb_zbs_zfhmin -mabi=lp64d"' \
 -DITERATIONS=1 -DMULTITHREAD=1 -DTOTAL_DATA_SIZE=2000 \
 core_list_join.c core_main.c core_matrix.c core_state.c core_util.c posix/core_portme.c \
 /work/configs/coremark/timing.c -Wl,--wrap=start_time -Wl,--wrap=stop_time \
 -o /build/guest/rootfs/bin/coremark
riscv64-linux-gnu-readelf -h -l /build/guest/rootfs/bin/coremark > /work/results/coremark/coremark-elf.txt
riscv64-linux-gnu-objdump -d /build/guest/rootfs/bin/coremark > /work/results/coremark/coremark-disassembly.txt
sha256sum /build/guest/rootfs/bin/coremark > /work/results/coremark/coremark.sha256
