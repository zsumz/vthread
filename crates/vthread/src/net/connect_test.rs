use crate::{Error, Runtime, net::TcpStream};

#[test]
fn refused_connect_returns_the_socket_error_instead_of_successful_readiness() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            let mut task = scope.spawn("refused", move || TcpStream::connect(address))?;
            assert!(matches!(task.join()?, Err(Error::Io(_))));
            Ok(())
        })
        .unwrap();
}
