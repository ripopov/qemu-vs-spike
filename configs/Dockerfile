ARG BASE_IMAGE=ubuntu:24.04
FROM ${BASE_IMAGE}
RUN apt-get update && apt-get install -y --no-install-recommends build-essential gcc-riscv64-linux-gnu binutils-riscv64-linux-gnu libc6-dev-riscv64-cross bc bison flex libssl-dev libelf-dev cpio curl ca-certificates xz-utils git python3 && rm -rf /var/lib/apt/lists/*
WORKDIR /work
