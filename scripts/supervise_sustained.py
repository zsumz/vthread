#!/usr/bin/env python3
"""Run and verify the release soak as one externally supervised process."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


MINIMUM_LIFETIMES = 10_000_000
UPDATES_PER_TASK = 8


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(candidate: Path, *arguments: str) -> str:
    return subprocess.check_output(
        ["git", "-C", str(candidate), *arguments], text=True
    ).strip()


def resource_sample(pid: int, start: float) -> dict[str, int | float | None]:
    sample: dict[str, int | float | None] = {
        "seconds": time.monotonic() - start,
        "rss_kib": None,
        "fds": None,
    }
    try:
        status = Path(f"/proc/{pid}/status").read_text()
        rss = next(line for line in status.splitlines() if line.startswith("VmRSS:"))
        sample["rss_kib"] = int(rss.split()[1])
        sample["fds"] = len(list(Path(f"/proc/{pid}/fd").iterdir()))
    except (OSError, StopIteration, ValueError):
        pass
    return sample


def validate_report(
    report: object,
    *,
    duration: int,
    carriers: int,
    tasks: int,
    wall_seconds: float,
    returncode: int,
    system: str,
    machine: str,
) -> list[str]:
    errors = []
    if not isinstance(report, dict):
        return ["report must be a JSON object"]
    required = {
        "schema",
        "workload",
        "connection_strategy",
        "carriers",
        "tasks",
        "iterations",
        "mutex_updates",
        "elapsed_ns",
        "spawned",
        "completed",
        "parks",
        "wakes",
        "stack_allocated",
        "stack_reused",
    }
    missing = sorted(required - report.keys())
    if missing:
        return [f"missing report fields: {', '.join(missing)}"]
    integer_fields = required - {"workload", "connection_strategy"}
    invalid = sorted(
        name
        for name in integer_fields
        if not isinstance(report[name], int) or isinstance(report[name], bool)
    )
    if invalid:
        return [f"invalid integer fields: {', '.join(invalid)}"]
    iterations = report["iterations"]
    expected_lifetimes = iterations * (tasks + 5)
    expected_updates = iterations * tasks * UPDATES_PER_TASK
    checks = (
        (returncode == 0, f"process exited {returncode}"),
        (system == "Linux", f"expected Linux, got {system}"),
        (machine == "x86_64", f"expected x86_64, got {machine}"),
        (report["schema"] == 1, "unexpected report schema"),
        (report["workload"] == "mixed-soak", "unexpected workload"),
        (
            report["connection_strategy"] == "persistent-pair",
            "unexpected connection strategy",
        ),
        (report["carriers"] == carriers, "carrier count mismatch"),
        (report["tasks"] == tasks, "task count mismatch"),
        (report["elapsed_ns"] >= duration * 1_000_000_000, "short internal run"),
        (wall_seconds >= duration, "short supervised run"),
        (report["spawned"] == expected_lifetimes, "spawn accounting mismatch"),
        (report["completed"] == expected_lifetimes, "completion accounting mismatch"),
        (report["completed"] >= MINIMUM_LIFETIMES, "lifetime threshold not met"),
        (report["parks"] == report["wakes"], "park/wake accounting mismatch"),
        (
            report["stack_allocated"] + report["stack_reused"]
            == report["spawned"],
            "stack acquisition accounting mismatch",
        ),
        (report["mutex_updates"] == expected_updates, "mutex accounting mismatch"),
    )
    errors.extend(message for passed, message in checks if not passed)
    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--duration", type=int, required=True)
    parser.add_argument("--carriers", type=int, required=True)
    parser.add_argument("--tasks", type=int, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    candidate = args.candidate.resolve()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    head = git(candidate, "rev-parse", "HEAD")
    dirty = bool(git(candidate, "status", "--porcelain"))
    if head != args.head or dirty:
        raise SystemExit(f"candidate identity mismatch: head={head} dirty={dirty}")
    command = [str(binary), "soak", str(args.duration), str(args.carriers), str(args.tasks)]
    started_at = datetime.now(timezone.utc).isoformat()
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        cwd=candidate,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    samples = []
    timeout = args.duration + 600
    while process.poll() is None:
        sample = resource_sample(process.pid, started)
        samples.append(sample)
        print(json.dumps({"process_sample": sample}, sort_keys=True), flush=True)
        try:
            process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            if time.monotonic() - started > timeout:
                process.kill()
                process.wait()
                break
    stdout, stderr = process.communicate()
    wall_seconds = time.monotonic() - started
    output.joinpath("stdout.log").write_text(stdout)
    output.joinpath("stderr.log").write_text(stderr)
    try:
        lines = [line for line in stdout.splitlines() if line.strip()]
        if len(lines) != 1:
            raise ValueError(f"expected one report line, got {len(lines)}")
        report = json.loads(lines[0])
        parse_error = None
    except (ValueError, json.JSONDecodeError) as error:
        report = {}
        parse_error = str(error)
    errors = [] if parse_error is None else [parse_error]
    if parse_error is None:
        errors.extend(
            validate_report(
                report,
                duration=args.duration,
                carriers=args.carriers,
                tasks=args.tasks,
                wall_seconds=wall_seconds,
                returncode=process.returncode,
                system=platform.system(),
                machine=platform.machine(),
            )
        )
    receipt = {
        "schema": 1,
        "status": "passed" if not errors else "failed",
        "candidate": {
            "head": head,
            "tree": git(candidate, "rev-parse", "HEAD^{tree}"),
            "dirty": dirty,
            "cargo_lock_sha256": sha256(candidate / "Cargo.lock"),
        },
        "control": {
            "commit": os.environ.get("GITHUB_SHA"),
            "run_id": os.environ.get("GITHUB_RUN_ID"),
            "repository": os.environ.get("GITHUB_REPOSITORY"),
        },
        "host": {
            "system": platform.system(),
            "machine": platform.machine(),
            "kernel": platform.release(),
            "cpu_count": os.cpu_count(),
        },
        "process": {
            "pid": process.pid,
            "attempts": 1,
            "command": command,
            "binary_sha256": sha256(binary),
            "started_at": started_at,
            "finished_at": datetime.now(timezone.utc).isoformat(),
            "wall_seconds": wall_seconds,
            "returncode": process.returncode,
            "samples": samples,
        },
        "contract": {
            "duration": args.duration,
            "carriers": args.carriers,
            "tasks": args.tasks,
            "minimum_lifetimes": MINIMUM_LIFETIMES,
        },
        "report": report,
        "errors": errors,
    }
    receipt_path = output / "receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    output.joinpath("receipt.sha256").write_text(f"{sha256(receipt_path)}  receipt.json\n")
    print(json.dumps({"status": receipt["status"], "report": report, "errors": errors}, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
