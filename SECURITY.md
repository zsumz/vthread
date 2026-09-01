# Security

## Reporting

Report sensitive findings through the access-controlled
[zsumz/vthread-private issue tracker](https://github.com/zsumz/vthread-private/issues).
Do not place exploit details, credentials, or private data in a public channel. If you do not
have repository access, contact the maintainer through GitHub without including sensitive
details so a private channel can be arranged.

Include the exact source revision, operating system and architecture, Rust and dependency
versions, configured limits, a minimal reproduction, and the expected impact. Call out any
borrowed lifetime, stack transition, cancellation, native call, FFI, or destructor behavior.

## Boundaries

vthread cannot make arbitrary FFI, native memory faults, stack overflow, panic-abort builds,
or unsupported targets safe. Cancellation is cooperative. Native calls and destructors cannot
be preempted. Standard-library blocking calls also block their carrier unless the application
delegates them through vthread's blocking boundary.
