use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{COOKIE_BLOCK_BITS, allocate_cookie};

#[test]
fn exhausted_cookie_blocks_never_advance_or_reissue_an_old_identity() {
    let exhausted = 1_u64 << (u64::BITS - COOKIE_BLOCK_BITS);
    let blocks = AtomicU64::new(exhausted);
    let next = Cell::new(0);
    for _ in 0..3 {
        assert!(catch_unwind(AssertUnwindSafe(|| allocate_cookie(&next, &blocks))).is_err());
        assert_eq!(blocks.load(Ordering::Relaxed), exhausted);
        assert_eq!(next.get(), 0);
    }
}

#[test]
fn the_last_cookie_is_returned_once_before_permanent_exhaustion() {
    let exhausted = 1_u64 << (u64::BITS - COOKIE_BLOCK_BITS);
    let blocks = AtomicU64::new(exhausted - 1);
    let next = Cell::new(0);
    assert_eq!(
        allocate_cookie(&next, &blocks),
        (exhausted - 1) << COOKIE_BLOCK_BITS
    );
    next.set(u64::MAX);
    assert_eq!(allocate_cookie(&next, &blocks), u64::MAX);
    assert_eq!(next.get(), 0);
    assert_eq!(blocks.load(Ordering::Relaxed), exhausted);
    assert!(catch_unwind(AssertUnwindSafe(|| allocate_cookie(&next, &blocks))).is_err());
    assert_eq!(blocks.load(Ordering::Relaxed), exhausted);
}
