# Contributing

Use Rust 1.96.1, zrail 0.0.3-rc.5, zcheck 0.0.2, and Python 3.11 or newer.

Keep source, caches, and build products on zdev. From the checkout:

```sh
export CARGO_HOME="$PWD/target/cargo-home"
export CARGO_TARGET_DIR="$PWD/target/build"
export TMPDIR="$PWD/target/tmp"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"
zcheck run check
```

Keep every Rust source file below 300 lines and give production files sibling `_test.rs`
modules. Add a failing regression before repairing scheduler or stack state. Preserve carrier
affinity, structured ownership, bounded resources, and explicit blocking boundaries.

Inspect every zrail grant and approve or deny it with a concrete reason. Use small PGP-signed
conventional commits with the `zsumz <shawn@zsumz.com>` identity, subject only, and no
coauthors. Keep the repository Apache-2.0 only and every package on the shared workspace version.
