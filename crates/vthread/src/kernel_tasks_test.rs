#[cfg(not(feature = "runtime-evidence"))]
use super::OwnedTask;

#[cfg(not(feature = "runtime-evidence"))]
#[test]
fn owned_task_fits_one_cache_line() {
    assert_eq!(std::mem::size_of::<OwnedTask>(), 64);
}
