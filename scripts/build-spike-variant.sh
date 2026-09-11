#!/bin/bash
# Build the current Spike worktree into build/spike-<name> with the non-PGO flags.
set -euo pipefail
cd "$(dirname "$0")/.."
name=${1:?usage: build-spike-variant.sh <name> [make args]}; shift || true
case "$name" in baseline|opt|pgo|train) echo "Refusing to overwrite the preserved build/spike-$name" >&2; exit 1;; esac
root=$PWD
llvm=/opt/homebrew/opt/llvm@20/bin
flags='-O3 -flto=thin'
mkdir -p "build/spike-$name"
cd "build/spike-$name"
if test ! -f Makefile; then
  "$root/spike/configure" --prefix="$root/build/install-$name" \
    --with-boost=/opt/homebrew --with-boost-libdir=/opt/homebrew/lib \
    CC="$llvm/clang" CXX="$llvm/clang++" \
    CFLAGS="$flags" CXXFLAGS="$flags" \
    LDFLAGS="$flags -L/opt/homebrew/lib" CPPFLAGS=-I/opt/homebrew/include
fi
if rg -q 'fprofile-(instr|generate|use)' Makefile; then
  echo 'Refusing to use a PGO-configured build directory' >&2
  exit 1
fi
make -j8 spike libriscv.a libdisasm.a libsoftfloat.a libfesvr.a libfdt.a "$@"
