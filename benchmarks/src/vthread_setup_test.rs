use super::{cpu_list, first_cpus};

#[test]
fn sparse_allowed_cpus_are_selected_without_oversubscribing() {
    assert_eq!(first_cpus("2-3,7,10-12", 4).unwrap(), vec![2, 3, 7, 10]);
    assert_eq!(first_cpus("0", 1).unwrap(), vec![0]);
    assert!(first_cpus("2-3,7", 4).is_err());
    assert!(first_cpus("0-3", 0).is_err());
}

#[test]
fn malformed_masks_are_not_silently_accepted() {
    for mask in [
        "", "-1", "3-1", "1,1", "2,1", "0-2,2-3", "0-1-2", "0,", "cpu",
    ] {
        assert!(first_cpus(mask, 1).is_err(), "accepted {mask}");
    }
}

#[test]
fn mask_expansion_is_bounded_by_the_requested_worker_count() {
    assert_eq!(
        first_cpus(&format!("0-{}", usize::MAX), 2).unwrap(),
        vec![0, 1]
    );
}

#[test]
fn readback_requires_the_kernel_cpu_list_field() {
    assert_eq!(
        cpu_list("Name:\tvthread-carrier\nCpus_allowed_list:\t2-3,7\n").unwrap(),
        "2-3,7"
    );
    assert!(cpu_list("Cpus_allowed:\tffff\n").is_err());
    assert!(cpu_list("Cpus_allowed_list:\t\n").is_err());
}
