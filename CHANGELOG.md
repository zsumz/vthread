# Changelog

## Unreleased

- Replaced the corosensei dependency with a native stack engine for Linux x86_64 and macOS
  ARM64 that owns guard-page-backed stack mappings, stack identities, and forced reclamation
  of suspended stacks. Fiber control blocks and entries live on the fiber's own stack, so a
  pooled fiber start performs no heap allocation.

## 0.0.2-rc.1 - 2026-09-01

- Added opt-in bounded runtime evidence with sequenced task, stack, wait, timer, queue, scope,
  and shutdown transitions.
- Added exact reusable wait generations, reusable stack identities, and explicit evidence-loss
  reporting for external qualification tools.
- Added an opt-in generation-bound probe that exercises the real wake selector and proves stale
  generations are rejected.

## 0.0.1 - 2026-09-01

- Added carrier-affine stackful virtual threads with structured scope ownership.
- Added borrowed local children, typed joins, cancellation, deadlines, and task-local values.
- Added bounded virtual synchronization primitives and MPMC channels.
- Added readiness-based TCP, UDP, and Unix sockets.
- Added bounded native delegation for blocking work, DNS, and filesystem operations.
- Added runtime diagnostics, stall policies, supervisors, and deadline-based shutdown.
- Added checks for the reference application and architecture rules.
- Enforced panic unwinding and support for Linux x86_64 and macOS ARM64.
