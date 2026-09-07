# Security

## Report privately

Report sensitive findings through the
[private vulnerability reporting form](https://github.com/zsumz/vthread/security/advisories/new).
Do not place exploit details, credentials, or private data in a public issue or discussion.

Include:

- The source revision, operating system and architecture.
- Rust and dependency versions, plus configured runtime limits.
- A minimal reproduction and the expected impact.
- Any relevant borrowed lifetime, stack transition, cancellation, native call,
  FFI or destructor behavior.

## Boundaries

- Arbitrary FFI, native memory faults and stack overflow are outside the runtime's
  safety guarantees. Panic-abort builds and unsupported targets are rejected.
- Cancellation is cooperative. Native calls and destructors cannot be preempted.
- Standard-library blocking calls block their carrier unless the application
  delegates them through vthread's blocking boundary.
