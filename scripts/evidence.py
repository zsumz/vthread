"""Capture source identity and service resource measurements."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def output(*command: str) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_digest() -> str:
    paths = output("git", "ls-files", "--cached", "--others", "--exclude-standard").splitlines()
    source = hashlib.sha256()
    for name in sorted(set(paths)):
        path = ROOT / name
        if path.suffix in {".rs", ".toml", ".lock", ".py", ".sh", ".yml"} and path.is_file():
            source.update(name.encode() + b"\0" + path.read_bytes() + b"\0")
    return source.hexdigest()


def metadata(context: str) -> dict:
    cpu = platform.processor()
    if platform.system() == "Darwin":
        cpu = output("sysctl", "-n", "machdep.cpu.brand_string")
    elif Path("/proc/cpuinfo").exists():
        cpu = next(
            (
                line.split(":", 1)[1].strip()
                for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith(("model name", "Hardware"))
            ),
            cpu,
        )
    return {
        "head": output("git", "rev-parse", "HEAD"),
        "dirty": bool(output("git", "status", "--porcelain")),
        "source_sha256": source_digest(),
        "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
        "rustc": output("rustc", "-Vv"),
        "system": platform.system(),
        "machine": platform.machine(),
        "kernel": platform.release(),
        "cpu_count": os.cpu_count(),
        "cpu_model": cpu,
        "execution_context": context,
        "ci_run": os.environ.get("GITHUB_RUN_ID"),
        "ci_repository": os.environ.get("GITHUB_REPOSITORY"),
        "rustflags": os.environ.get("RUSTFLAGS", ""),
        "started_unix": time.time(),
    }


def resources(pid: int, start: float) -> dict:
    record = {"seconds": time.monotonic() - start, "rss_kib": None, "fds": None}
    try:
        record["rss_kib"] = int(
            subprocess.check_output(
                ["ps", "-o", "rss=", "-p", str(pid)],
                text=True,
                stderr=subprocess.DEVNULL,
            )
        )
    except (subprocess.CalledProcessError, ValueError):
        pass
    if platform.system() == "Linux":
        try:
            record["fds"] = len(list(Path(f"/proc/{pid}/fd").iterdir()))
        except OSError:
            pass
    return record
