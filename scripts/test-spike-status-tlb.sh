#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode=${1:-opt}
case "$mode" in baseline|opt) ;; *) echo 'Expected baseline or opt' >&2; exit 2;; esac
build_dir="$PWD/build/spike-$mode"
out="$PWD/build/optimization/test-status-$mode"
/opt/homebrew/opt/llvm@20/bin/clang++ -std=c++2a -O3 -flto=thin \
  -I"$build_dir" -Ispike/riscv -Ispike/fesvr -Ispike/softfloat \
  -I/opt/homebrew/include tests/spike-status-tlb.cc \
  "$build_dir/libriscv.a" "$build_dir/libdisasm.a" \
  "$build_dir/libsoftfloat.a" "$build_dir/libfesvr.a" "$build_dir/libfdt.a" \
  -L/opt/homebrew/lib -lboost_regex -lpthread -o "$out"
"$out"
