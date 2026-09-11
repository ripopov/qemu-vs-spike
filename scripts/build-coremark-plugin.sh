#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
/opt/homebrew/opt/llvm@20/bin/clang -O3 -shared -fPIC -undefined dynamic_lookup \
 $(pkg-config --cflags glib-2.0) -I qemu/include/plugins scripts/coremark-count.c \
 -o build/libcoremark-count.dylib $(pkg-config --libs glib-2.0)
