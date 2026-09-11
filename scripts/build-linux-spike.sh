#!/bin/bash
# Build the three report revisions without altering the Spike submodule checkout.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
cc=${CC:-clang}
cxx=${CXX:-clang++}
linker=${LINKER:-lld-21}
boost=${BOOST_ROOT:-$root/build/deps/root/usr}
jobs=${JOBS:-12}
mkdir -p results/intel-265k build/sources
for entry in baseline:4ffd6ba860f4190ceac2716fa3c2cf139e85538f opt:388e10aa247a52ce1b072ce7e477cdf5b521158f opt3:52bf14fe51902cadbd8b1404ba6ac87ab0e59e14; do
    name=${entry%%:*}
    revision=${entry#*:}
    source_dir="$root/build/sources/spike-$name"
    mkdir -p "$source_dir" "build/spike-$name"
    if [ ! -f "$source_dir/configure" ]; then
        git -C spike archive "$revision" | tar -x -C "$source_dir"
    fi
    (
        cd "build/spike-$name"
        if [ ! -f Makefile ]; then
            "$source_dir/configure" --prefix="$root/build/install-$name" \
                --with-boost="$boost" --with-boost-libdir="$boost/lib/x86_64-linux-gnu" \
                CC="$cc" CXX="$cxx" CFLAGS='-O3 -flto=thin' CXXFLAGS='-O3 -flto=thin' \
                CPPFLAGS="-I$boost/include" \
                LDFLAGS="-O3 -flto=thin -fuse-ld=$linker -L$boost/lib/x86_64-linux-gnu -Wl,-rpath,$boost/lib/x86_64-linux-gnu"
        fi
        if rg -q 'fprofile-(instr|generate|use)' Makefile; then
            echo 'Refusing a PGO-configured build' >&2
            exit 1
        fi
        make -j"$jobs" spike libriscv.a libdisasm.a libsoftfloat.a libfesvr.a libfdt.a
    ) > "results/intel-265k/spike-$name-build.log" 2>&1
done
