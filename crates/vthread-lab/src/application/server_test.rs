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
