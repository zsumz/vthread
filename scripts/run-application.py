#!/usr/bin/env python3
"""Run public-API service load and failure scenarios with native socket clients."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

import application_faults
import application_load
import application_verify
import evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--carriers", type=int, nargs="+", default=[1, 4])
    parser.add_argument("--concurrency", type=int, nargs="+", default=[1, 16, 64, 256])
    parser.add_argument("--rounds", type=int, default=128)
    parser.add_argument("--fault-rounds", type=int, default=3)
    parser.add_argument("--context", default="local uncontrolled application observation")
    args = parser.parse_args()
    try:
        coverage = application_verify.matrix(vars(args))
    except AssertionError:
        parser.error("application bounds exceeded, empty or repeated matrix entry")
    target = Path(os.environ.get("CARGO_TARGET_DIR", evidence.ROOT / "target"))
    if not target.is_absolute():
        parser.error("CARGO_TARGET_DIR must be an absolute build directory")
    if args.out is None:
        parent = evidence.ROOT / "target/application"
        parent.mkdir(parents=True, exist_ok=True)
        args.out = Path(tempfile.mkdtemp(prefix="check-", dir=parent)) / "run"
    args.out.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    receipt = dict(schema=1, status="failed", coverage=coverage, arguments=dict(vars(args)), cases=[])
    receipt["arguments"]["out"] = str(args.out)
    try:
        receipt["environment"] = evidence.metadata(args.context)
        binary = target / "release/vthread-application"
        with (args.out / "build.log").open("w") as log:
            subprocess.run(["cargo", "build", "--locked", "--release", "-p", "vthread-lab",
                            "--bin", "vthread-application"], cwd=evidence.ROOT, check=True,
                           stdout=log, stderr=subprocess.STDOUT, timeout=600)
        receipt["binary_sha256"] = evidence.digest(binary)
        for carriers in args.carriers:
            for concurrency in args.concurrency:
                assert time.monotonic() - started < 1800, "application run exceeded 30 minutes"
                name = f"load-{carriers}-{concurrency}"
                print(f"RUN {name}", flush=True)
                application_load.run(binary, args.out / name, carriers, concurrency, args.rounds)
                receipt["cases"].append(dict(kind="load", path=f"{name}/load.json"))
            for round_index in range(args.fault_rounds):
                assert time.monotonic() - started < 1800, "application run exceeded 30 minutes"
                name = f"faults-{carriers}-{round_index}"
                print(f"RUN {name}", flush=True)
                application_faults.run(binary, args.out / name, carriers)
                receipt["cases"].append(dict(kind="faults", carriers=carriers,
                                             round=round_index, path=f"{name}/faults.json"))
        assert evidence.source_digest() == receipt["environment"]["source_sha256"]
        receipt["status"] = "passed"
    except Exception as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    receipt["wall_seconds"] = time.monotonic() - started
    receipt["files"] = {str(path.relative_to(args.out)): evidence.digest(path)
                        for path in sorted(args.out.rglob("*")) if path.is_file()}
    path = args.out / "receipt.json"
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    if receipt["status"] == "passed":
        try:
            application_verify.verify(path, current=True)
        except Exception as error:
            receipt.update(status="failed", error=f"verification failed: {error}")
            path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"{receipt['status']}: {path}", flush=True)
    return receipt["status"] != "passed"


if __name__ == "__main__":
    sys.exit(main())
