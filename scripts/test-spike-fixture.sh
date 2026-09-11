#!/bin/bash
# Build and run a native regression fixture against build/spike-<variant> libraries.
set -euo pipefail
cd "$(dirname "$0")/.."
fixture=${1:?usage: test-spike-fixture.sh <tests/name.cc> <variant>}
variant=${2:?usage: test-spike-fixture.sh <tests/name.cc> <variant>}
build_dir="$PWD/build/spike-$variant"
mkdir -p build/round2
out="$PWD/build/round2/$(basename "$fixture" .cc)-$variant"
/opt/homebrew/opt/llvm@20/bin/clang++ -std=c++2a -O3 -flto=thin ${FIXTURE_FLAGS:-} \
  -I"$build_dir" -Ispike/riscv -Ispike/fesvr -Ispike/softfloat \
  -I/opt/homebrew/include "$fixture" \
  "$build_dir/libriscv.a" "$build_dir/libdisasm.a" \
  "$build_dir/libsoftfloat.a" "$build_dir/libfesvr.a" "$build_dir/libfdt.a" \
  -L/opt/homebrew/lib -lboost_regex -lpthread -o "$out"
"$out"
