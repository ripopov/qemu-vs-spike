#!/bin/bash
set -euo pipefail
WORKLOAD=${WORKLOAD:-boot}
RESULTS_DIR=/work/results
OUTPUT_DIR=/work/guest
if test "$WORKLOAD" = coremark; then
 RESULTS_DIR=/work/results/coremark
 OUTPUT_DIR=/work/guest/coremark
elif test "$WORKLOAD" != boot; then
 echo "Unknown WORKLOAD: $WORKLOAD" >&2; exit 2
fi
mkdir -p "$RESULTS_DIR" "$OUTPUT_DIR" /build/guest
cd /build/guest
fetch() { test -f "$2" || curl -fL --retry 3 "$1" -o "$2"; }
fetch https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.47.tar.xz linux.tar.xz
fetch https://busybox.net/downloads/busybox-1.37.0.tar.bz2 busybox.tar.bz2
fetch https://codeload.github.com/riscv-software-src/opensbi/tar.gz/refs/tags/v1.7 opensbi.tar.gz
test -d linux-6.12.47 || tar xf linux.tar.xz
test -d busybox-1.37.0 || tar xf busybox.tar.bz2
test -d opensbi-1.7 || tar xf opensbi.tar.gz
export ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu-
mkdir -p rootfs/{bin,dev,proc,sys}
cd busybox-1.37.0
make allnoconfig
sed -i 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/; s/# CONFIG_ASH is not set/CONFIG_ASH=y/; s/# CONFIG_SH_IS_ASH is not set/CONFIG_SH_IS_ASH=y/; s/# CONFIG_ECHO is not set/CONFIG_ECHO=y/; s/# CONFIG_POWEROFF is not set/CONFIG_POWEROFF=y/; s/# CONFIG_HALT is not set/CONFIG_HALT=y/' .config
make oldconfig </dev/null
make -j8
cp busybox ../rootfs/bin/
cd ..
ln -sf busybox rootfs/bin/sh
ln -sf busybox rootfs/bin/poweroff
cat > rootfs/init <<'INIT'
#!/bin/sh
/bin/busybox echo BENCH_BUSYBOX_READY
exec /bin/poweroff -f
INIT
if test "$WORKLOAD" = coremark; then
 bash /work/scripts/build-coremark.sh
 COREMARK_ITERATIONS=${COREMARK_ITERATIONS:-32000}
 COREMARK_SEED=${COREMARK_SEED:-0}
 [[ "$COREMARK_ITERATIONS" =~ ^[1-9][0-9]*$ ]] || exit 2
 [[ "$COREMARK_SEED" = 0 || "$COREMARK_SEED" = 0x3415 ]] || exit 2
 printf '{"iterations": %s, "seed": "%s"}\n' "$COREMARK_ITERATIONS" "$COREMARK_SEED" > "$RESULTS_DIR/workload.json"
 cat > rootfs/init <<INIT
#!/bin/sh
/bin/busybox echo BENCH_BUSYBOX_READY
/bin/coremark "$COREMARK_SEED" "$COREMARK_SEED" 0x66 "$COREMARK_ITERATIONS"
status=\$?
/bin/busybox echo BENCH_COREMARK_EXIT=\$status
exec /bin/poweroff -f
INIT
fi
chmod +x rootfs/init
cat > initramfs.list <<'CPIO'
dir /bin 755 0 0
dir /dev 755 0 0
dir /proc 755 0 0
dir /sys 755 0 0
nod /dev/console 600 0 0 c 5 1
nod /dev/null 666 0 0 c 1 3
file /bin/busybox /build/guest/rootfs/bin/busybox 755 0 0
slink /bin/sh busybox 777 0 0
slink /bin/poweroff busybox 777 0 0
file /init /build/guest/rootfs/init 755 0 0
CPIO
if test "$WORKLOAD" = coremark; then
 echo 'file /bin/coremark /build/guest/rootfs/bin/coremark 755 0 0' >> initramfs.list
fi
cd linux-6.12.47
make tinyconfig
scripts/config --enable 64BIT --enable MMU --enable RISCV_SBI --enable NONPORTABLE --disable SMP --enable BINFMT_ELF --enable BINFMT_SCRIPT --enable BLK_DEV_INITRD --enable PRINTK --enable TTY --enable HVC_DRIVER --enable HVC_RISCV_SBI --enable SERIAL_8250 --enable SERIAL_8250_CONSOLE --enable SERIAL_OF_PLATFORM --enable DEVTMPFS --enable DEVTMPFS_MOUNT --enable RISCV_TIMER --enable CLINT_TIMER --enable POWER_RESET --enable POWER_RESET_SYSCON --enable RISCV_SBI_V01 --enable SYSVIPC --enable FUTEX --enable FPU --enable RISCV_ISA_C --enable RISCV_ISA_ZICBOM --enable RISCV_ISA_ZICBOZ --set-str INITRAMFS_SOURCE /build/guest/initramfs.list --set-str CMDLINE 'console=hvc0 earlycon=sbi rdinit=/init nokaslr lpj=1000000 quiet' --enable CMDLINE_FORCE
make olddefconfig
make -j8 Image
cp .config "$RESULTS_DIR/linux.config"
cp arch/riscv/boot/Image /build/guest/Image
cd ../opensbi-1.7
make -j8 O=build-fixed PLATFORM=generic FW_PAYLOAD_PATH=/build/guest/Image FW_OPTIONS=0 FW_TEXT_START=0x80000000
cp build-fixed/platform/generic/firmware/fw_payload.elf /build/guest/
cp build-fixed/platform/generic/firmware/fw_jump.bin /build/guest/
cp ../busybox-1.37.0/.config "$RESULTS_DIR/busybox.config"
sha256sum /build/guest/Image /build/guest/fw_payload.elf /build/guest/fw_jump.bin /build/guest/*.tar.* > "$RESULTS_DIR/guest-sha256.txt"
riscv64-linux-gnu-gcc --version > "$RESULTS_DIR/guest-compiler.txt"

cp /build/guest/{Image,fw_payload.elf,fw_jump.bin,initramfs.list} "$OUTPUT_DIR/"
if test "$WORKLOAD" = coremark; then cp /build/guest/rootfs/bin/coremark "$OUTPUT_DIR/"; fi
