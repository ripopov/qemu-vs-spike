#!/bin/bash
# Run the existing retained-optimization fixtures against the Linux build.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
boost=${BOOST_ROOT:-$root/build/deps/root/usr}
cxx=${CXX:-clang++}
linker=${LINKER:-lld-21}
source_dir="$root/build/sources/spike-opt3"
build_dir="$root/build/spike-opt3"
mkdir -p build/intel-265k results/intel-265k
for fixture in spike-fetch-regime spike-status-tlb spike-console-poll; do
    "$cxx" -std=c++2a -O3 -flto=thin -fuse-ld="$linker" -DTEST_ADDRESS_FENCE \
        -I"$build_dir" -I"$source_dir/riscv" -I"$source_dir/fesvr" \
        -I"$source_dir/softfloat" -I"$boost/include" "tests/$fixture.cc" \
        "$build_dir/libriscv.a" "$build_dir/libdisasm.a" \
        "$build_dir/libsoftfloat.a" "$build_dir/libfesvr.a" "$build_dir/libfdt.a" \
        -L"$boost/lib/x86_64-linux-gnu" -Wl,-rpath,"$boost/lib/x86_64-linux-gnu" \
        -lboost_regex -lpthread -ldl -o "build/intel-265k/$fixture"
    "build/intel-265k/$fixture" > "results/intel-265k/$fixture.log" 2>&1
done
