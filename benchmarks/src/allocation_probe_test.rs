use super::{ALLOCATOR, begin, finish};
use std::{
    alloc::{GlobalAlloc, Layout},
    process::Command,
};

#[test]
fn allocation_window_counts_explicit_allocations_in_isolation() {
    const CHILD: &str = "VTHREAD_BENCHMARK_ALLOCATION_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "allocation_probe::allocation_probe_test::allocation_window_counts_explicit_allocations_in_isolation",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let layout = Layout::from_size_align(128, 16).unwrap();
    begin();
    // SAFETY: the nonzero layout is valid and the returned allocation is freed below.
    let pointer = unsafe { ALLOCATOR.alloc(layout) };
    assert!(!pointer.is_null());
    // SAFETY: pointer came from this allocator with the identical layout.
    unsafe { ALLOCATOR.dealloc(pointer, layout) };
    let counts = finish();
    assert_eq!(counts.allocations, 1);
    assert_eq!(counts.deallocations, 1);
    assert_eq!(counts.allocated_bytes, 128);
}
