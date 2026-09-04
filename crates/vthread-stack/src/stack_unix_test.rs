use super::{enable, page_size, release, reserve};

#[test]
fn the_page_size_is_a_power_of_two_of_at_least_four_kib() {
    let size = page_size().expect("page size");
    assert!(size.is_power_of_two());
    assert!(size >= 4096);
}

#[test]
fn enabled_pages_are_writable_and_release_succeeds() {
    let page = page_size().unwrap();
    let mapping = reserve(2 * page).unwrap();
    // SAFETY: the second page lies inside the reservation made above.
    unsafe { enable(mapping.add(page), page).unwrap() };
    // SAFETY: the second page was just enabled and belongs to this test.
    unsafe {
        mapping.add(page).write_volatile(0xA5);
        assert_eq!(mapping.add(page).read_volatile(), 0xA5);
        assert_eq!(mapping.add(2 * page - 1).read_volatile(), 0);
    }
    // SAFETY: nothing references the mapping after this point.
    unsafe { release(mapping, 2 * page).unwrap() };
}

#[test]
fn empty_reservations_are_rejected() {
    assert!(reserve(0).is_err());
}
