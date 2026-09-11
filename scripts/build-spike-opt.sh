#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
llvm=/opt/homebrew/opt/llvm@20/bin
flags='-O3 -flto=thin'
mkdir -p build/spike-opt
cd build/spike-opt
if test ! -f Makefile; then
  "$root/spike/configure" --prefix="$root/build/install-opt" \
    --with-boost=/opt/homebrew --with-boost-libdir=/opt/homebrew/lib \
    CC="$llvm/clang" CXX="$llvm/clang++" \
    CFLAGS="$flags" CXXFLAGS="$flags" \
    LDFLAGS="$flags -L/opt/homebrew/lib" CPPFLAGS=-I/opt/homebrew/include
fi
if rg -q 'fprofile-(instr|generate|use)' Makefile; then
  echo 'Refusing to use a PGO-configured build directory' >&2
  exit 1
fi
make -j6 spike
