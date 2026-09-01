# AGENTS.md

## Product rules

- Every package stays at version `0.0.1` and uses the Apache License 2.0 only.
- A started virtual thread never migrates between carrier threads.
- The public `vthread` crate contains no unsafe Rust.
- Unsafe stack mechanics stay inside `vthread-stack` and require a `SAFETY:` comment.
- Runtime queues, admission, wake permits, timers, native work, and channels remain bounded.
- Structured scopes are the default ownership model; detached work is never implicit.
- Standard-library blocking calls are never presented as transparently virtualized.
- Every parked or yielded task has an operator-visible reason.
- One park generation has exactly one selected winner.
- Timer and remote-ready events carry the generation they intend to wake.

## Development rules

- Keep Rust source files below 300 lines.
- Give every production Rust file a sibling `_test.rs` module.
- Add a regression test before repairing scheduler or stack state.
- Prefer small state machines and typed transitions over clever synchronization.
- Do not add another runtime or an async executor to the core dependency graph.
- Do not call `thread::sleep` outside the carrier timer driver.
- Run `zcheck run check` before proposing a change.
- Inspect every zrail grant and explicitly approve or deny it with a reason.
- Commit as `zsumz <shawn@zsumz.com>` with a PGP-signed conventional subject, no body, and no coauthor.
