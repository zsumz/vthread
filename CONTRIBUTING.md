# Contributing

## Set up

Use Rust **1.96.1**, zrail **0.0.3-rc.5**, zcheck **0.0.2**, and Python **3.11+**.
The repository installer verifies checksums for the pinned zrail and zcheck tools.

From the checkout:

```sh
rustup toolchain install 1.96.1 --profile minimal --component clippy,rustfmt
scripts/install-guardrails.sh
export PATH="$HOME/.local/bin:$PATH"

cargo fetch --locked
cargo fetch --locked --manifest-path reference/Cargo.toml
cargo fetch --locked --manifest-path benchmarks/Cargo.toml
zcheck run check
```

The canonical check covers native debug/release tests, all-feature tests, docs,
Clippy, architecture policy, application smoke tests and the standalone benchmark
harness. Set `CARGO_TARGET_DIR` to an absolute local path to use a separate build cache.

## Keep changes small

- Keep Rust files below 300 lines, with sibling `_test.rs` modules for production files.
- Add a failing regression before repairing scheduler or stack state.
- Preserve carrier affinity, structured ownership, bounded resources and explicit blocking boundaries.
- Inspect every zrail grant; approve or deny it with a concrete reason.
- Keep packages on the shared workspace version and use Apache-2.0 only.

## Commit and qualify

Use small PGP-signed Conventional Commits: one concise subject, no body and no
coauthor trailers. Maintainer commits use `zsumz <shawn@zsumz.com>`.

Run `zcheck run check` before submitting changes. The [Release workflow](RELEASE.md) requires both-target CI before zrelease
packages and rehearses the workspace. Push a `release/**` branch to practice;
actual publication requires an explicit tagged dispatch and release approval.
