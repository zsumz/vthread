use super::{WakeClock, WakeStamp, encode_nanoseconds};

#[test]
fn encoding_reserves_zero_for_an_empty_stamp() {
    assert_eq!(encode_nanoseconds(0), 1);
    assert_eq!(encode_nanoseconds(41), 42);
    assert_eq!(encode_nanoseconds(u128::MAX), u64::MAX);
}

#[test]
fn a_published_stamp_is_consumed_once() {
    let clock = WakeClock::new();
    let stamp = WakeStamp::new();
    clock.publish(&stamp);
    let _ = clock.elapsed(&stamp);

    let second = std::panic::catch_unwind(|| clock.elapsed(&stamp));
    assert!(second.is_err());
}

#[test]
fn a_consumed_stamp_can_be_reused() {
    let clock = WakeClock::new();
    let stamp = WakeStamp::new();
    for _ in 0..3 {
        clock.publish(&stamp);
        let _ = clock.elapsed(&stamp);
    }
}
