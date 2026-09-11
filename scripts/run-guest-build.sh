#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p results guest
IMAGE=${BUILDER_IMAGE:-rv-boot-builder}
if test -z "${BUILDER_IMAGE:-}"; then
 docker build -t "$IMAGE" -f configs/Dockerfile .
fi
docker run --rm -e WORKLOAD="${WORKLOAD:-boot}" -e COREMARK_ITERATIONS="${COREMARK_ITERATIONS:-32000}" -e COREMARK_SEED="${COREMARK_SEED:-0}" -v "$PWD:/work" -v qemu-vs-spike-guest-build:/build "$IMAGE" bash /work/scripts/build-guest.sh
