use crate::{Error, Runtime, ScopeOptions, SuspensionReason, context, net::unix::UnixStream};
use std::{
    os::fd::AsFd,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[test]
fn blocked_io_yields_to_runnable_work_before_registering_readiness() {
    let runtime = Runtime::new().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    let admitted = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    runtime
        .run_scope(|scope| {
            let start = Arc::clone(&admitted);
            let mut read = scope.spawn("read", move || {
                while !start.load(Ordering::Acquire) {
                    crate::yield_now()?;
                }
                reader.read_exact(&mut [0; 1])
            })?;
            let start = Arc::clone(&admitted);
            let sampled = Arc::clone(&observed);
            let mut write = scope.spawn("write", move || {
                while !start.load(Ordering::Acquire) {
                    crate::yield_now()?;
                }
                sampled.store(context_readiness_waits(), Ordering::Release);
                writer.write_all(b"x")
            })?;
            admitted.store(true, Ordering::Release);
            write.join()??;
            read.join()??;
            Ok(())
        })
        .unwrap();
    assert_eq!(observed.load(Ordering::Acquire), 0);
}

#[test]
fn blocked_io_eventually_registers_readiness_under_runnable_load() {
    let runtime = Runtime::new().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    runtime
        .run_scope(|scope| {
            let mut read = scope.spawn("read", move || reader.read_exact(&mut [0; 1]))?;
            let mut write = scope.spawn("write", move || {
                let mut registered = false;
                for _ in 0..=super::RUNNABLE_YIELD_LIMIT {
                    if context_readiness_waits() != 0 {
                        registered = true;
                        break;
                    }
                    crate::yield_now()?;
                }
                assert!(
                    registered,
                    "blocked I/O did not reach the readiness reactor"
                );
                writer.write_all(b"x")
            })?;
            write.join()??;
            read.join()??;
            Ok(())
        })
        .unwrap();
}

fn context_readiness_waits() -> usize {
    let mounted = context::current().unwrap();
    mounted
        .execution()
        .unwrap()
        .shared()
        .services
        .get()
        .unwrap()
        .snapshot()
        .readiness_waits()
}

#[test]
fn immediately_ready_operation_does_not_enter_a_suspension_reason() {
    let runtime = Runtime::new().unwrap();
    let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("ready", move || {
                    super::operation(
                        socket.as_fd(),
                        zio::Interest::READABLE,
                        SuspensionReason::IoRead,
                        || {
                            let mounted = context::current().unwrap();
                            assert_eq!(
                                mounted.execution().unwrap().data.reason(),
                                SuspensionReason::Park
                            );
                            Ok::<_, std::io::Error>(())
                        },
                    )
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
}

#[test]
fn socket_deadline_cleans_registration_and_preserves_unread_data() {
    let runtime = Runtime::new().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    let reader = Arc::new(reader);
    let error = runtime
        .run_scope_with(
            ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(30)),
            |scope| {
                let reader = Arc::clone(&reader);
                let mut task = scope.spawn("timeout", move || reader.read(&mut [0; 1]))?;
                assert!(matches!(task.join()?, Err(Error::DeadlineExceeded)));
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(error.primary(), Error::DeadlineExceeded));
    runtime
        .run_scope(|scope| {
            scope
                .spawn("write", move || writer.write_all(b"x"))?
                .join()??;
            let mut task = scope.spawn("read-again", move || {
                let mut byte = [0; 1];
                reader.read_exact(&mut byte)?;
                Ok::<_, Error>(byte)
            })?;
            assert_eq!(task.join()??, *b"x");
            Ok(())
        })
        .unwrap();
}

#[test]
fn readiness_capacity_rejects_excess_waits_without_poisoning_the_socket() {
    use crate::support_test::until;
    let runtime = Runtime::builder().io_capacity(1).build().unwrap();
    let (reader, _writer) = UnixStream::pair().unwrap();
    let reader = Arc::new(reader);
    runtime
        .run_scope(|scope| {
            let socket = Arc::clone(&reader);
            let mut first = scope.spawn("first", move || socket.read(&mut [0; 1]))?;
            until(|| runtime.snapshot().services.readiness_waits == 1);
            let socket = Arc::clone(&reader);
            let mut second = scope.spawn("excess", move || socket.read(&mut [0; 1]))?;
            assert!(matches!(
                second.join()?,
                Err(Error::Capacity {
                    resource: crate::error::CapacityResource::Readiness,
                    limit: 1
                })
            ));
            scope.cancel();
            assert!(matches!(first.join()?, Err(Error::Cancelled)));
            Ok(())
        })
        .unwrap();
    until(|| runtime.snapshot().services.readiness_registered == 0);
}

#[test]
fn read_exact_reports_short_eof_without_discarding_the_received_prefix() {
    use std::net::Shutdown;
    let runtime = Runtime::new().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    runtime.run_scope(|scope| {
        scope.spawn("write", move || {
            writer.write_all(b"ab")?;
            writer.shutdown(Shutdown::Write)
        })?.join()??;
        scope.spawn("short-read", move || {
            let mut buffer = [0; 4];
            assert!(matches!(reader.read_exact(&mut buffer), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof));
            assert_eq!(&buffer[..2], b"ab");
        })?.join()?;
        Ok(())
    }).unwrap();
}
