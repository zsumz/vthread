# vthread-stack

The stack and context-switching backend used by `vthread`.

Applications should depend on `vthread`, whose public crate forbids unsafe Rust. This package
contains the narrow stack boundary, pins corosensei, and enforces the rule that every started
stack is resumed, unwound, and reclaimed by its owning carrier.

The package is internal to the workspace and is licensed under the Apache License 2.0.
