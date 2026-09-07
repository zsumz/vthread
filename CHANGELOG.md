# Changelog

## Unreleased

## 0.0.2 - 2026-09-07

- Prepared the retained `0.0.2-rc.2` runtime for early evaluation as `0.0.2`.
- Updated all package versions and exact internal dependency pins together.
- Switched installation examples to crates.io and kept qualification limitations
  explicit in [release status](RELEASE.md).

## 0.0.2-rc.2 - 2026-09-07

### Runtime

- Replaced corosensei with native guarded stacks on Linux x86_64 and macOS ARM64.
  Stack-resident control blocks and entries make pooled fiber starts allocation-free.
- Reused carrier-owned task, execution, stack and synchronization-wait storage; kept ready
  entries compact and started tasks permanently carrier-affine.
- Added cancellation-safe direct mutex ownership transfer without a second native value lock.
- Bounded ready-wake priority and remote-admission service; deferred incomplete wake publication
  and cleanup without occupying a carrier on a paused publisher.
- Routed expired timers directly, gated borrowed-scope maintenance by revocation epochs, and
  cached visible ingress while preserving bounded admission and shutdown ownership.

### Correctness

- Made ready-queue cleanup inspect every entry exactly once.
- Prevented mounted-context borrows from surviving their resume through suspension.
- Preserved the caller's mount when dropping an unstarted fiber.
- Made forced-unwind identity exhaustion permanent, even after caught panics.

### Qualification

- Added opt-in scheduler and handoff diagnostics, ordered concurrency regressions and modeled
  ownership/publication protocols; native debug and release tests are mandatory qualification.
- Kept a standalone runtime-only benchmark harness and added its formatting, lint and test
  checks to canonical qualification. Experimental reports remain outside the release tree.

See [release status](RELEASE.md) for qualification results and open gates.

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
