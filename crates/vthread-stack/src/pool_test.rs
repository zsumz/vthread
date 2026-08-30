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
