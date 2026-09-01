#!/usr/bin/env python3
"""Prove both crate boundaries reject panic-abort builds."""

from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = (
    ("vthread-stack", "vthread-stack requires panic=unwind"),
    ("vthread", "vthread requires panic=unwind"),
)


def main():
    for package, diagnostic in CONTRACTS:
        result = subprocess.run(
            ["cargo", "rustc", "--locked", "-p", package, "--lib", "--", "-C", "panic=abort"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=300,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            print(output, file=sys.stderr)
            raise AssertionError(f"{package} did not reject panic=abort with its contract message")
    print("panic-abort builds rejected at both crate boundaries")


if __name__ == "__main__":
    main()
