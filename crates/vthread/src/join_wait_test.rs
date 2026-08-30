use crate::{Runtime, park_pair, support_test::until};

#[test]
fn a_virtual_join_parks_and_releases_the_single_carrier_for_other_work() {
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            let (parker, waker) = park_pair();
            let first = scope.spawn("target", move || {
                parker.park().unwrap();
                42
            })?;
            let joining = scope.spawn("joiner", move || first.join())?;
            until(|| scope.snapshot().parked == 2);
            let ready = scope.spawn("other work", move || waker.unpark())?;
            ready.join()?;
            assert_eq!(joining.join()??, 42);
            Ok(())
        })
        .unwrap();
}
