# vthread-sync-core

The narrow unsafe exclusive-value core used by `vthread` synchronization.

Applications should depend on `vthread`. This support crate exists so the public runtime can
forbid unsafe Rust while its virtual mutex uses a linear ownership capability instead of a second
native mutex. Queueing, cancellation, bounds, and scheduling remain in the safe runtime crate.

This support crate has no compatibility contract for direct downstream use. It is licensed under
the Apache License 2.0.
