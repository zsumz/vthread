use super::*;

#[test]
fn an_abandoned_source_releases_the_command_gate() {
    let (park, wake) = vthread::parking::park_pair();
    let shared = Shared::new(
        Config {
            mode: Mode::Local,
            iterations: 1,
            pin: false,
        },
        wake,
    );
    drop(Stop(&shared));
    assert!(shared.stop.load(Ordering::Acquire));
    // Consume the abandonment notification on a real mounted task. Without the
    // notification this command park cannot return normally.
    let runtime = vthread::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("released command", move || park.park())?
                .join()??;
            Ok(())
        })
        .unwrap();
}

#[test]
fn invalid_work_counts_do_not_start_a_runtime() {
    for iterations in [0, 100_001] {
        assert!(
            crate::mutex_handoff::run(Config {
                mode: Mode::Local,
                iterations,
                pin: false
            })
            .is_err()
        );
    }
}
