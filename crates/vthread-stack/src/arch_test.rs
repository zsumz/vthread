use super::{FRAME_LEN, init_frame};
use crate::{MappedStack, STACK_ALIGNMENT, context::FiberCore};

#[test]
fn the_first_frame_sits_directly_below_the_frame_top() {
    let stack = MappedStack::new(64 * 1024, 0).expect("allocate stack");
    // SAFETY: the mapping is fresh, so nothing is live on it.
    let placement = unsafe { FiberCore::place(&stack, || {}) };
    // SAFETY: the placement leaves a whole frame plus headroom below its frame top.
    let sp = unsafe { init_frame(placement.frame_top, placement.core) };
    assert_eq!(placement.frame_top - sp, FRAME_LEN);
    assert_eq!(placement.frame_top % STACK_ALIGNMENT, 0);
    assert!(sp > stack.limit().get() + stack.guard_len());
    assert!(
        FRAME_LEN <= stack.guard_len(),
        "one page must always hold the first frame"
    );
}
