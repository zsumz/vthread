#!/usr/bin/env python3
"""Repository checks that express vthread-specific architecture rules."""

from __future__ import annotations

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
PUBLIC = ROOT / "crates" / "vthread" / "src"
STACK = ROOT / "crates" / "vthread-stack" / "src"
LAB = ROOT / "crates" / "vthread-lab" / "src"
REFERENCE = ROOT / "reference" / "src"
MAX_LINES = 300
PUBLIC_DEPENDENCIES = {"libc", "socket2", "vthread-stack", "zio"}


def rust_sources(root: pathlib.Path) -> list[pathlib.Path]:
    return sorted(path for path in root.rglob("*.rs") if not path.name.endswith("_test.rs"))


def relative(path: pathlib.Path) -> str:
    return path.relative_to(ROOT).as_posix()


def check_line_limits(errors: list[str]) -> None:
    for path in sorted((ROOT / "crates").rglob("*.rs")) + sorted(REFERENCE.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) > MAX_LINES:
            errors.append(f"{relative(path)} has {len(lines)} lines; hard limit is {MAX_LINES}")


def check_sibling_tests(errors: list[str]) -> None:
    for path in rust_sources(PUBLIC) + rust_sources(STACK) + rust_sources(LAB) + rust_sources(REFERENCE):
        sibling = path.with_name(f"{path.stem}_test.rs")
        if not sibling.is_file():
            errors.append(f"{relative(path)} is missing sibling test {relative(sibling)}")
        source = path.read_text(encoding="utf-8")
        expected = f'#[path = "{sibling.name}"]'
        if expected not in source:
            errors.append(f"{relative(path)} does not include {sibling.name} with #[path]")


def check_unsafe_boundary(errors: list[str]) -> None:
    token = re.compile(r"\bunsafe\b")
    for path in list(PUBLIC.rglob("*.rs")) + list(REFERENCE.rglob("*.rs")):
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if token.search(line) and "forbid(unsafe_code)" not in line:
                errors.append(f"{relative(path)}:{number} contains unsafe outside stack backend")

    for path in STACK.rglob("*.rs"):
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if re.search(r"\bunsafe\s*\{", line):
                context = "\n".join(lines[max(0, index - 2) : index + 1])
                if "SAFETY:" not in context:
                    errors.append(f"{relative(path)}:{index + 1} unsafe block lacks SAFETY comment")


def check_core_dependencies(errors: list[str]) -> None:
    manifest = ROOT / "crates" / "vthread" / "Cargo.toml"
    parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
    dependencies = set(parsed.get("dependencies", {}))
    unexpected = sorted(dependencies - PUBLIC_DEPENDENCIES)
    if unexpected:
        errors.append(
            f"{relative(manifest)} contains unreviewed core dependencies: {', '.join(unexpected)}"
        )


def check_blocking_boundaries(errors: list[str]) -> None:
    allowed = {PUBLIC / "kernel_drive.rs"}
    blocking = re.compile(r"\b(?:std::)?thread::sleep\s*\(")
    for path in rust_sources(PUBLIC):
        if path in allowed:
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if blocking.search(line):
                errors.append(
                    f"{relative(path)}:{number} blocks a carrier outside the timer driver"
                )


def main() -> int:
    errors: list[str] = []
    check_line_limits(errors)
    check_sibling_tests(errors)
    check_unsafe_boundary(errors)
    check_core_dependencies(errors)
    check_blocking_boundaries(errors)

    if errors:
        print("vthread guardrail policy failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("vthread guardrail policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
