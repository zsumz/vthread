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
SYNC_CORE = ROOT / "crates" / "vthread-sync-core" / "src"
LAB = ROOT / "crates" / "vthread-lab" / "src"
REFERENCE = ROOT / "reference" / "src"
BENCHMARKS = ROOT / "benchmarks" / "src"
MAX_LINES = 300
PUBLIC_DEPENDENCIES = {
    "crossbeam-queue",
    "libc",
    "socket2",
    "vthread-stack",
    "vthread-sync-core",
    "zio",
}


def rust_sources(root: pathlib.Path) -> list[pathlib.Path]:
    return sorted(path for path in root.rglob("*.rs") if not path.name.endswith("_test.rs"))


def relative(path: pathlib.Path) -> str:
    return path.relative_to(ROOT).as_posix()


def check_line_limits(errors: list[str]) -> None:
    for path in sorted((ROOT / "crates").rglob("*.rs")) + sorted(REFERENCE.rglob("*.rs")) + sorted(BENCHMARKS.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) > MAX_LINES:
            errors.append(f"{relative(path)} has {len(lines)} lines; hard limit is {MAX_LINES}")


def check_sibling_tests(errors: list[str]) -> None:
    roots = (PUBLIC, STACK, SYNC_CORE, LAB, REFERENCE, BENCHMARKS)
    for path in (path for root in roots for path in rust_sources(root)):
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

    for path in list(STACK.rglob("*.rs")) + list(SYNC_CORE.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if re.search(r"\bunsafe\s*(?:\{|impl\b)", line):
                context = "\n".join(lines[max(0, index - 2) : index + 1])
                if "SAFETY:" not in context:
                    errors.append(f"{relative(path)}:{index + 1} unsafe block lacks SAFETY comment")


def check_core_dependencies(errors: list[str]) -> None:
    manifest = ROOT / "crates" / "vthread" / "Cargo.toml"
    parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
    dependencies = set(parsed.get("dependencies", {}))
    if "vthread-stack" not in dependencies:
        errors.append("vthread must retain the native vthread-stack dependency")
    if parsed.get("features", {}).get("default") != []:
        errors.append("default vthread qualification requires uninstrumented features")
    unexpected = sorted(dependencies - PUBLIC_DEPENDENCIES)
    if unexpected:
        errors.append(
            f"{relative(manifest)} contains unreviewed core dependencies: {', '.join(unexpected)}"
        )


def check_native_qualification(errors: list[str], tasks: dict) -> None:
    command = ["cargo", "test", "--locked", "--workspace", "--all-targets"]
    for name, expected in (
        ("test-native", command),
        ("test-native-release", [*command, "--release"]),
    ):
        if tasks.get(name, {}).get("run") != expected:
            errors.append(f"{name} must run the exact default-feature workspace test command")
    if "test-native-release" not in tasks.get("check", {}).get("needs", []):
        errors.append("check must require test-native-release")
    if "test-native" not in tasks.get("test-native-release", {}).get("needs", []):
        errors.append("test-native-release must require test-native")
    if "test" not in tasks.get("test-native", {}).get("needs", []):
        errors.append("test-native must follow all-feature tests, not overlap their execution")


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


def check_benchmark_qualification(errors: list[str], tasks: dict) -> None:
    manifest = ["--manifest-path", "benchmarks/Cargo.toml"]
    checks = (
        ("benchmark-format", ["cargo", "fmt", *manifest, "--", "--check"], "application-smoke"),
        ("benchmark-clippy", ["cargo", "clippy", "--locked", *manifest, "--all-targets", "--all-features", "--", "-D", "warnings"], "benchmark-format"),
        ("benchmark-test", ["cargo", "test", "--locked", *manifest, "--all-targets"], "benchmark-clippy"),
        ("benchmark-test-features", ["cargo", "test", "--locked", *manifest, "--all-targets", "--all-features"], "benchmark-test"),
    )
    for name, command, dependency in checks:
        task = tasks.get(name, {})
        if task.get("run") != command or dependency not in task.get("needs", []):
            errors.append(f"{name} must retain its standalone command and ordered dependency")
    if "benchmark-test-features" not in tasks.get("check", {}).get("needs", []):
        errors.append("check must require standalone benchmark qualification")


def check_history_performance(errors: list[str], tasks: dict) -> None:
    expected = [
        "cargo", "test", "--locked", "-p", "vthread", "--release",
        "cancellation::cancellation_history_test::sequential_dynamic_history_retains_its_performance_guard",
        "--", "--ignored", "--exact", "--nocapture",
    ]
    if tasks.get("perf-cancellation-history", {}).get("run") != expected:
        errors.append("perf-cancellation-history must retain its explicit optimized timing guard")


def check_release_qualification(errors: list[str], workflow: str) -> None:
    job = workflow.partition("  qualification:\n")[2]
    job = re.split(r"^  \S", job, maxsplit=1, flags=re.MULTILINE)[0]
    steps = {}
    for step in re.split(r"^      - name: ", job, flags=re.MULTILINE)[1:]:
        name, _, body = step.partition("\n")
        steps[name] = body
    application = steps.get("Qualify application", "")
    command = " ".join(application.partition("        run: >-\n")[2].split())
    expected = (
        "python3 scripts/run-application.py --out .qualification/application "
        "--offered-rates 2000 --offered-count 256 "
        '--context "GitHub release qualification ${{ matrix.target }}"'
    )
    if command != expected or "        if:" in application:
        errors.append("release qualification must retain the full application and offered-load matrix")
    package = steps.get("Verify workspace packages", "")
    expected_package = (
        "        shell: bash\n"
        "        run: |\n"
        "          set -euo pipefail\n"
        "          mkdir -p .qualification/package\n"
        "          cargo package --locked --offline --workspace --exclude vthread-lab "
        "2>&1 | tee .qualification/package/verification.log\n"
    )
    if (package != expected_package or "Qualify application" not in steps
            or list(steps).index("Verify workspace packages") <= list(steps).index("Qualify application")):
        errors.append("release qualification must verify workspace packages offline after the application")
    upload = steps.get("Upload qualification evidence", "")
    if ("            .qualification/\n" not in upload
            or "            ${{ env.CARGO_TARGET_DIR }}/package/*.crate\n" not in upload):
        errors.append("release qualification must upload application evidence, package logs and archives")


def main() -> int:
    errors: list[str] = []
    check_line_limits(errors)
    check_sibling_tests(errors)
    check_unsafe_boundary(errors)
    check_core_dependencies(errors)
    config = tomllib.loads((ROOT / "zcheck.toml").read_text(encoding="utf-8"))
    check_native_qualification(errors, config["tasks"])
    check_history_performance(errors, config["tasks"])
    check_benchmark_qualification(errors, config["tasks"])
    check_release_qualification(errors, (ROOT / ".github/workflows/ci.yml").read_text())
    check_blocking_boundaries(errors)

    if errors:
        print("vthread guardrail policy failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("vthread guardrail policy passed")
    print("qualification engine=vthread-stack configuration=default profiles=debug,release")
    print("qualification engine=vthread-stack configuration=all-features profile=debug")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
