#[test]
fn framed_service_rejects_bad_input_and_still_accepts_healthy_clients() {
    use std::io::{Read, Write};
    let runtime = vthread::Runtime::new().unwrap();
    let owner = runtime
        .supervisor(vthread::ScopeOptions::default())
        .unwrap();
    let service = super::start(&owner, 2, 2, std::time::Duration::from_secs(2)).unwrap();
    let mut bad = std::net::TcpStream::connect(service.address).unwrap();
    bad.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let mut greeting = [0];
    bad.read_exact(&mut greeting).unwrap();
    assert_eq!(greeting, [super::protocol::READY]);
    bad.write_all(&[0; 16]).unwrap();
    assert_eq!(bad.read(&mut greeting).unwrap(), 0);
    let mut good = std::net::TcpStream::connect(service.address).unwrap();
    good.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    good.read_exact(&mut greeting).unwrap();
    let mut frame = Vec::new();
    frame.extend(2u32.to_be_bytes());
    frame.extend(4u32.to_be_bytes());
    frame.extend(7u64.to_be_bytes());
    frame.extend([1, 2]);
    good.write_all(&frame).unwrap();
    let mut bytes = [0; 12];
    good.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes[..8], &7u64.to_be_bytes());
    assert_eq!(&bytes[8..], &super::protocol::response(&[1, 2], 4, 7));
    drop(good);
    runtime.shutdown().unwrap();
    service.join().unwrap();
    owner.shutdown().unwrap();
}

#[test]
fn combined_shutdown_body_and_cancellation_are_expected() {
    for body in [vthread::Error::RuntimeStopped, vthread::Error::Cancelled] {
        let result = join_result(move || {
            vthread::local_scope(|scope| {
                scope.cancel();
                Err(body)
            })
        });
        assert!(
            result.is_ok(),
            "legitimate combined shutdown error: {result:?}"
        );
    }
}

#[test]
fn shutdown_cancellation_does_not_hide_an_unexpected_body() {
    let error = join_result(|| {
        vthread::local_scope(|scope| {
            scope.cancel();
            Err(vthread::Error::WouldBlock)
        })
    })
    .unwrap_err();
    let failure = error.scope_failure().unwrap();
    assert!(matches!(failure.body(), Some(vthread::Error::WouldBlock)));
    assert!(matches!(failure.policy(), Some(vthread::Error::Cancelled)));
}

#[test]
fn shutdown_body_does_not_hide_unobserved_child_panics() {
    for count in [1, 2] {
        let error = join_result(move || {
            vthread::local_scope(|scope| {
                let mut children = Vec::new();
                for _ in 0..count {
                    children.push(scope.spawn("failed-request", || panic!("request panic"))?);
                }
                while !children.iter().all(|child| child.is_finished()) {
                    vthread::yield_now()?;
                }
                drop(children);
                scope.cancel();
                Err(vthread::Error::RuntimeStopped)
            })
        })
        .unwrap_err();
        let failure = error.scope_failure().unwrap();
        assert!(matches!(
            failure.body(),
            Some(vthread::Error::RuntimeStopped)
        ));
        assert!(matches!(failure.policy(), Some(vthread::Error::Cancelled)));
        assert!(matches!(
            failure.child(),
            Some(vthread::Error::TaskPanicked { .. })
        ));
        assert_eq!(failure.additional_child_failures(), count - 1);
    }
}

#[test]
fn a_panicking_service_worker_is_not_a_clean_shutdown() {
    let error = join_result(|| panic!("service worker panic")).unwrap_err();
    assert!(matches!(error, vthread::Error::TaskPanicked { .. }));
}

fn join_result(body: impl FnOnce() -> vthread::Result<()> + Send + 'static) -> vthread::Result<()> {
    let runtime = vthread::Runtime::new().unwrap();
    let owner = runtime
        .supervisor(vthread::ScopeOptions::default())
        .unwrap();
    let task = owner.spawn("service-outcome", body).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "fixture synchronization timeout"
        );
        std::thread::yield_now();
    }
    // Match the real application ordering, with a deterministic completed outcome.
    runtime.shutdown().unwrap();
    let service = super::Service {
        address: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        counts: Default::default(),
        tasks: vec![task],
    };
    let result = service.join();
    owner.shutdown().unwrap();
    result
}
