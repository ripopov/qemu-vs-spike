#!/usr/bin/env python3
"""Build OxySpike with a Linux-boot profile; CoreMark is held out of training."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name", help="fresh variant name")
    parser.add_argument("--profdata", default="llvm-profdata-22")
    parser.add_argument("--target-cpu", help="optional rustc CPU target, e.g. native")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-zA-Z0-9_-]+", args.name):
        parser.error("name must contain only letters, digits, underscores or hyphens")
    root = Path(__file__).resolve().parent.parent
    work = root / "build" / "oxyspike-pgo" / args.name
    output = root / "build" / "oxyspike-variants" / args.name
    if output.exists():
        parser.error(f"refusing to overwrite {output}")
    work.mkdir(parents=True, exist_ok=False)
    raw = work / "raw"
    raw.mkdir()
    commands = []
    cpu_flags = ["-C", f"target-cpu={args.target_cpu}"] if args.target_cpu else []

    def run(command, *, flags=None, log=None, timeout=300):
        env = os.environ.copy()
        env.pop("RUSTFLAGS", None)
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        if flags is not None:
            env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
        commands.append({"argv": list(map(str, command)), "rustflags": flags})
        with (work / (log or f"command-{len(commands)}.log")).open("w") as stream:
            subprocess.run(command, cwd=root, env=env, stdout=stream,
                           stderr=subprocess.STDOUT, check=True, timeout=timeout)

    cargo = ["cargo", "build", "--offline", "--release", "--locked",
             "--manifest-path", str(root / "oxyspike/Cargo.toml")]
    run(cargo + ["--target-dir", str(work / "train")],
        flags=cpu_flags + ["-C", f"profile-generate={raw}"], log="build-train.log")
    trainer = work / "train/release/oxyspike"
    for index in range(3):
        log = f"train-{index}.log"
        run([str(trainer), "--dtb", str(root / "results/platform.dtb"),
             str(root / "guest/fw_payload.elf")], log=log)
        text = (work / log).read_text()
        if "BENCH_BUSYBOX_READY" not in text or "reboot: Power down" not in text:
            raise RuntimeError("Linux training did not reach readiness and shutdown")
    profiles = sorted(raw.glob("*.profraw"))
    if not profiles:
        raise RuntimeError("training produced no profiles")
    merged = work / "boot.profdata"
    run([args.profdata, "merge", "-o", str(merged), *map(str, profiles)])
    run(cargo + ["--target-dir", str(work / "use")],
        flags=cpu_flags + ["-C", f"profile-use={merged}", "-C",
               "llvm-args=-pgo-warn-missing-function"], log="build-use.log")
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(work / "use/release/oxyspike", output)
    inputs = sorted((root / "oxyspike/src").glob("*.rs")) + [
        root / "oxyspike/Cargo.toml", root / "oxyspike/Cargo.lock",
        root / "guest/fw_payload.elf", root / "results/platform.dtb", merged, output]
    metadata = {
        "training": "three complete Linux boots; no CoreMark training",
        "rustc": subprocess.check_output(["rustc", "-vV"], text=True),
        "profdata": subprocess.check_output([args.profdata, "--version"], text=True),
        "commands": commands,
        "sha256": {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                   for p in inputs},
    }
    (work / "provenance.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
