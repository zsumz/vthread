use crate::{Error, Runtime, net};

#[test]
fn hostname_resolution_is_explicit_bounded_native_work() {
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            let addresses = scope
                .spawn("resolve", || net::lookup_host("localhost", 80, 64))?
                .join()??;
            assert!(!addresses.is_empty());
            assert!(addresses.iter().all(|address| address.port() == 80));
            Ok(())
        })
        .unwrap();
    assert!(net::lookup_host("localhost", 80, 0).is_err());
    assert!(matches!(
        net::lookup_host("localhost", 80, 64),
        Err(Error::OutsideVThread)
    ));
}
