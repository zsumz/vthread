use super::StackPool;

#[test]
fn completed_stacks_are_reused() {
    let mut pool = StackPool::new(128 * 1024, 1);
    let first = pool.acquire().expect("allocate first stack");
    pool.release(first);
    let second = pool.acquire().expect("reuse first stack");
    drop(second);

    let snapshot = pool.snapshot();
    assert_eq!(snapshot.allocated, 1);
    assert_eq!(snapshot.reused, 1);
    assert_eq!(snapshot.cached, 0);
}

#[test]
fn the_cache_never_exceeds_its_limit() {
    let mut pool = StackPool::new(128 * 1024, 1);
    let first = pool.acquire().expect("allocate first stack");
    let second = pool.acquire().expect("allocate second stack");
    pool.release(first);
    pool.release(second);

    let snapshot = pool.snapshot();
    assert_eq!(snapshot.cached, 1);
    assert_eq!(snapshot.retained, 1);
    assert_eq!(snapshot.discarded, 1);
}

#[test]
#[cfg(feature = "runtime-evidence")]
fn mapping_identity_survives_cache_reuse_and_retires_on_discard() {
    let mut pool = StackPool::new(64 * 1024, 1);
    let (first, stack) = pool.acquire_identified().unwrap();
    assert!(pool.release_identified(first, stack));
    let (reused, stack) = pool.acquire_identified().unwrap();
    assert_eq!(reused, first);
    drop(stack);
    assert!(pool.retire(reused));
    assert!(!pool.retire(reused));
}

#[test]
#[cfg(feature = "runtime-evidence")]
fn mismatched_release_does_not_retire_an_unrelated_mapping() {
    let mut pool = StackPool::new(64 * 1024, 1);
    let (first, first_stack) = pool.acquire_identified().unwrap();
    let (second, second_stack) = pool.acquire_identified().unwrap();
    assert!(!pool.release_identified(second, first_stack));
    assert!(pool.release_identified(second, second_stack));
    assert!(!pool.retire(first));
}

#[test]
#[cfg(feature = "runtime-evidence")]
fn allocations_carry_pool_local_identities_in_order() {
    let mut pool = StackPool::new(64 * 1024, 2);
    let (first, first_stack) = pool.acquire_identified().unwrap();
    let (second, second_stack) = pool.acquire_identified().unwrap();
    assert_eq!((first, second), (1, 2));
    assert_eq!(first_stack.identity(), first);
    assert_eq!(second_stack.identity(), second);
}

#[test]
#[cfg(feature = "runtime-evidence")]
fn stacks_issued_by_another_pool_are_discarded() {
    let mut owner = StackPool::new(64 * 1024, 1);
    let mut other = StackPool::new(64 * 1024, 1);
    let (identity, stack) = owner.acquire_identified().unwrap();
    assert!(!other.release_identified(identity, stack));
    assert_eq!(other.snapshot().discarded, 1);
    assert_eq!(other.snapshot().cached, 0);
    assert!(owner.retire(identity), "the owner still tracks its mapping");
}
