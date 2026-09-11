#!/usr/bin/env python3
"""Supervise one mixed-runtime process and retain source-bound resource evidence."""

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import subprocess
import time

import evidence
import sustained_verify


def sample(pid, started):
    result = evidence.resources(pid, started)
    if platform.system() == "Darwin":
        # lsof includes cwd, executable and mapped files; only count numeric descriptors.
        listing = subprocess.run(["lsof", "-nP", "-a", "-p", str(pid), "-Ff"],
                                 capture_output=True, text=True, timeout=5, check=True)
        result["fds"] = sum(line[1:].isdigit() for line in listing.stdout.splitlines()
                            if line.startswith("f"))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=3600)
    parser.add_argument("--carriers", type=int, choices=(1, 4), required=True)
    parser.add_argument("--tasks", type=int, default=4096)
    parser.add_argument("--smoke", action="store_true", help="short harness check; never release qualification")
    args = parser.parse_args()
    if not 30 <= args.duration <= 86400 or not 2 <= args.tasks <= 4096:
        parser.error("invalid workload bounds")
    if not args.smoke and (args.duration < 3600 or args.tasks != 4096):
        parser.error("production qualification requires at least one hour and 4096 tasks")
    environment = evidence.metadata("sustained production scope" if not args.smoke else "harness smoke")
    if environment["dirty"]:
        parser.error("qualification requires a clean committed source tree")
    binary = args.binary.resolve(strict=True)
    binary_hash = evidence.digest(binary)
    args.out.mkdir(parents=True, exist_ok=False)
    interval = 1 if args.smoke else sustained_verify.SAMPLE_SECONDS
    command = [str(binary), "soak", str(args.duration), str(args.carriers), str(args.tasks)]
    started_at = datetime.now(timezone.utc).isoformat()
    started = time.monotonic()
    samples, collection_errors, timed_out = [], [], False
    with (args.out / "stdout.log").open("w") as stdout, (args.out / "stderr.log").open("w") as stderr:
        process = subprocess.Popen(command, cwd=evidence.ROOT, stdout=stdout, stderr=stderr)
        try:
            while process.poll() is None:
                if time.monotonic() - started > args.duration + 120:
                    timed_out = True
                    process.kill()
                    break
                try:
                    sampled_at = time.monotonic()
                    record = sample(process.pid, started)
                    if process.poll() is not None:
                        break
                    samples.append(record)
                    with (args.out / "samples.jsonl").open("a") as output:
                        output.write(json.dumps(record) + "\n")
                    print(json.dumps(record), flush=True)
                except (OSError, subprocess.SubprocessError) as error:
                    # A process may exit between poll and sample. Missing live samples fail closed.
                    if process.poll() is None:
                        collection_errors.append(str(error))
                try:
                    process.wait(timeout=max(0.001, interval - (time.monotonic() - sampled_at)))
                except subprocess.TimeoutExpired:
                    pass
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
    wall = time.monotonic() - started
    report = {}
    try:
        lines = (args.out / "stdout.log").read_text().splitlines()
        if len(lines) != 1:
            raise ValueError("expected exactly one completed workload report")
        report = json.loads(lines[0])
    except (ValueError, json.JSONDecodeError) as error:
        collection_errors.append(str(error))
    receipt = dict(schema=1, status="failed", environment=environment,
                   host=dict(system=platform.system(), machine=platform.machine()),
                   contract=dict(duration=args.duration, carriers=args.carriers, tasks=args.tasks,
                                 smoke=args.smoke, sample_seconds=interval,
                                 minimum_lifetimes=sustained_verify.MINIMUM_LIFETIMES),
                   process=dict(pid=process.pid, attempts=1, command=command,
                                binary_sha256=binary_hash, started_at=started_at,
                                wall_seconds=wall, returncode=process.returncode,
                                timed_out=timed_out, samples=samples), report=report,
                   source_unchanged=(evidence.source_digest() == environment["source_sha256"]
                                     and evidence.output("git", "rev-parse", "HEAD") == environment["head"]
                                     and not evidence.output("git", "status", "--porcelain")
                                     and evidence.digest(binary) == binary_hash))
    errors, resource_summary = sustained_verify.validate(receipt)
    errors.extend(collection_errors)
    if (args.out / "stderr.log").stat().st_size:
        errors.append("unexpected workload stderr")
    receipt.update(status="failed" if errors else "passed", errors=errors, resources=resource_summary)
    receipt["files"] = {p.name: evidence.digest(p) for p in args.out.iterdir() if p.is_file()}
    path = args.out / "receipt.json"
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    (args.out / "receipt.sha256").write_text(evidence.digest(path) + "  receipt.json\n")
    print(json.dumps(dict(status=receipt["status"], report=report, errors=errors)), flush=True)
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
