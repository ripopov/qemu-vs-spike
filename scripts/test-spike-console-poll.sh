#!/bin/bash
# Build and run the console polling fixture against build/spike-<variant>/libfesvr.a.
set -euo pipefail
cd "$(dirname "$0")/.."
variant=${1:?usage: test-spike-console-poll.sh <variant>}
mkdir -p build/round2
out="$PWD/build/round2/spike-console-poll-$variant"
/opt/homebrew/opt/llvm@20/bin/clang++ -std=c++2a -O2 -Ispike/fesvr tests/spike-console-poll.cc "build/spike-$variant/libfesvr.a" -lpthread -o "$out"
"$out"
