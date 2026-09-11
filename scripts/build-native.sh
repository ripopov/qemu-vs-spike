#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
MODE=${1:-baseline}
LLVM=/opt/homebrew/opt/llvm@20/bin
export PATH="$LLVM:/opt/homebrew/bin:$PATH"
export CC="$LLVM/clang" CXX="$LLVM/clang++"
FLAGS='-O3 -flto=thin'
case "$MODE" in
 baseline) ;;
 train) FLAGS="$FLAGS -fprofile-instr-generate=$ROOT/results/%m-%p.profraw" ;;
 pgo) FLAGS="$FLAGS -fprofile-instr-use=$ROOT/results/spike.profdata" ;;
 *) exit 2;;
esac
mkdir -p "build/spike-$MODE" "build/qemu-$MODE"
cd "build/spike-$MODE"
if test ! -f Makefile; then
 "$ROOT/spike/configure" --prefix="$ROOT/build/install-$MODE" --with-boost=/opt/homebrew --with-boost-libdir=/opt/homebrew/lib CC="$CC" CXX="$CXX" CFLAGS="$FLAGS" CXXFLAGS="$FLAGS" LDFLAGS="$FLAGS -L/opt/homebrew/lib" CPPFLAGS=-I/opt/homebrew/include
fi
make -j6 spike
if test "$MODE" = pgo; then FLAGS="-O3 -flto=thin -fprofile-instr-use=$ROOT/results/qemu.profdata"; fi
cd "$ROOT/build/qemu-$MODE"
if test ! -f build.ninja; then
 "$ROOT/qemu/configure" --target-list=riscv64-softmmu --cc="$CC" --cxx="$CXX" --extra-cflags="$FLAGS" --extra-ldflags="$FLAGS" --enable-lto --disable-werror --disable-docs --disable-tools --disable-user --disable-guest-agent --disable-slirp --disable-capstone --disable-gtk --disable-sdl --disable-cocoa --enable-plugins --disable-gio
fi
ninja -j6 qemu-system-riscv64
