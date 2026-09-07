# vthread-stack

The stack and context-switching backend used by `vthread`.

Applications should depend on [`vthread`](https://github.com/zsumz/vthread), whose public crate
forbids unsafe Rust. This support crate isolates the unsafe stack mechanics and has no
compatibility contract for direct downstream use.

Version `0.0.2` is prepared for early development and evaluation; crates.io publication is
pending. See the [release status and limitations](https://github.com/zsumz/vthread/blob/release/0.0.2/RELEASE.md).

## Ownership and safety

- Guard pages protect stack mappings. Context switching supports Linux x86_64 and macOS ARM64.
- Every started stack is resumed, unwound, and reclaimed by its owning carrier.
- A fiber's control block and entry live on its own stack; starting one on a pooled stack
  requires no allocation.
- Unwinding panics are required. A persistent cleanup mount failure during lexical scope exit
  aborts the process because borrowed executable stacks cannot outlive their environment.

[Apache License 2.0](LICENSE)
