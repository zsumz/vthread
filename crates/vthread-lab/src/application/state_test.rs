#[test]
fn activated_connections_release_their_counts() {
    let state = super::Shared::default();
    let runtime = vthread::Runtime::new().unwrap();
    let listener = vthread::net::TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::net::TcpStream::connect(address).unwrap();
    runtime
        .run_scope(|scope| {
            let socket = scope.spawn("accept", move || listener.accept())?.join()??.0;
            let mut connection = super::Connection::new(socket, state.clone());
            assert_eq!(super::change(&state, |s| s.pending), 1);
            connection.activate();
            assert_eq!(super::change(&state, |s| s.active), 1);
            drop(connection);
            let counts = super::change(&state, |s| *s);
            assert_eq!(
                (
                    counts.accepted,
                    counts.closed,
                    counts.pending,
                    counts.active
                ),
                (1, 1, 0, 0)
            );
            Ok(())
        })
        .unwrap();
    drop(peer);
}
