#!/usr/bin/env python3
"""Build a vendored source ZIP and test its reference app after verified extraction."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = "SOURCE-MANIFEST.json"
MAX_UNCOMPRESSED_BYTES = 384 * 1024 * 1024


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verify_extract(archive: Path, destination: Path, expected_digest: str) -> dict:
    """Reject corrupt inventories and path escapes before writing any archive content."""
    if digest(archive.read_bytes()) != expected_digest:
        raise ValueError("archive checksum mismatch")
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or len(names) > 25_000:
            raise ValueError("duplicate or excessive archive entries")
        if sum(entry.file_size for entry in entries) > MAX_UNCOMPRESSED_BYTES:
            raise ValueError("source archive exceeds size limit")
        for entry in entries:
            path = PurePosixPath(entry.filename)
            mode = entry.external_attr >> 16
            if (entry.filename in ("", ".") or path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or path.as_posix() != entry.filename or entry.is_dir()
                    or stat.S_IFMT(mode) not in (0, stat.S_IFREG)):
                raise ValueError("non-regular or escaping archive path")
        manifest = json.loads(zipped.read(MANIFEST))
        if manifest.get("schema") != 1 or set(names) != set(manifest["files"]) | {MANIFEST}:
            raise ValueError("source inventory mismatch")
        for name, expected in manifest["files"].items():
            if digest(zipped.read(name)) != expected:
                raise ValueError(f"source hash mismatch: {name}")
        destination.mkdir(exist_ok=False)
        for entry in entries:
            path = destination / entry.filename
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(zipped.read(entry.filename))
            path.chmod(0o755 if entry.external_attr >> 16 & 0o111 else 0o644)
        return manifest


def run(command: list[str], cwd: Path, log: Path, env: dict | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600)
    log.write_text(json.dumps(command) + "\n" + result.stderr + result.stdout)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}); inspect {log}")
    return result.stdout


def package(output: Path) -> dict:
    tree = output / "source"
    tree.mkdir()
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    for name in sorted(set(names) - {""}):
        source = ROOT / name
        if not source.is_file():
            continue
        if source.is_symlink():
            raise ValueError(f"source symlink requires an explicit distribution decision: {name}")
        target = tree / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for crate in ("vthread", "vthread-stack"):
        listing = run(["cargo", "package", "--locked", "--allow-dirty", "--list", "-p", crate],
                      ROOT, output / f"{crate}-contents.log").splitlines()
        for required in ("README.md", "LICENSE"):
            if required not in listing:
                raise ValueError(f"{crate} package omits {required}")
            if required == "LICENSE" and (ROOT / required).read_bytes() != (
                    ROOT / "crates" / crate / required).read_bytes():
                raise ValueError(f"{crate} license differs from repository license")
    config = run(["cargo", "vendor", "--locked", "--offline", "--versioned-dirs",
                  "--sync", "reference/Cargo.toml",
                  str(tree / "vendor")], ROOT, output / "vendor.log")
    # Cargo emits one directory source. Make it portable relative to .cargo/config.toml.
    lines = ["directory = \"vendor\"" if line.startswith("directory = ") else line
             for line in config.splitlines()]
    (tree / ".cargo").mkdir(exist_ok=True)
    (tree / ".cargo/config.toml").write_text("\n".join(lines) + "\n")
    files = sorted(path for path in tree.rglob("*") if path.is_file())
    manifest = {
        "schema": 1,
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
        "kind": "vendored-source-distribution",
        "distribution": "vendored source with locked dependencies",
        "files": {str(path.relative_to(tree)): digest(path.read_bytes()) for path in files},
    }
    archive = output / "vthread-source.zip"
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as zipped:
        for path in files:
            zipped.write(path, path.relative_to(tree))
        zipped.writestr(MANIFEST, json.dumps(manifest, indent=2) + "\n")
    archive_digest = digest(archive.read_bytes())
    (output / "vthread-source.zip.sha256").write_text(archive_digest + "  vthread-source.zip\n")
    extracted = output / "extracted"
    verify_extract(archive, extracted, archive_digest)
    return {"archive_sha256": archive_digest, "source": manifest, "extracted": str(extracted)}


def qualify(output: Path, receipt: dict) -> None:
    extracted = Path(receipt["extracted"])
    manifest = "reference/Cargo.toml"
    environment = os.environ.copy()
    environment["CARGO_NET_OFFLINE"] = "true"
    build_parent = Path(environment.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    if not build_parent.is_absolute():
        raise ValueError("source-package builds require an absolute build directory")
    build_parent = build_parent.resolve()
    build_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="vthread-reference-build-", dir=build_parent) as build:
        environment["CARGO_TARGET_DIR"] = build
        metadata = json.loads(run(["cargo", "metadata", "--locked", "--offline", "--format-version=1",
                                   "--manifest-path", manifest], extracted, output / "metadata.log", environment))
        for dependency in metadata["packages"]:
            if not Path(dependency["manifest_path"]).resolve().is_relative_to(extracted.resolve()):
                raise ValueError(f"reference app escaped the extracted distribution: {dependency['name']}")
        for phase, arguments in (
            ("format", ["fmt", "--manifest-path", manifest, "--", "--check"]),
            ("clippy", ["clippy", "--manifest-path", manifest, "--locked", "--offline", "--all-targets", "--", "-D", "warnings"]),
            ("test", ["test", "--manifest-path", manifest, "--locked", "--offline"]),
            ("run", ["run", "--manifest-path", manifest, "--locked", "--offline"]),
        ):
            run(["cargo", *arguments], extracted, output / f"reference-{phase}.log", environment)
    receipt.update({"status": "passed", "platform": platform.platform(),
                    "machine": platform.machine(), "all_dependencies_inside_archive": True})


def main() -> None:
    base = ROOT / "target/distributions"
    base.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="source-", dir=base))
    print(f"distribution evidence: {output}", flush=True)
    receipt = package(output)
    qualify(output, receipt)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"verified source distribution and reference app: {output / 'receipt.json'}")


if __name__ == "__main__":
    main()
