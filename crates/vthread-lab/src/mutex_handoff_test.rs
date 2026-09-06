use super::*;

#[test]
fn foreign_owner_and_missing_owner_evidence_are_rejected() {
    assert!(check_owners(Mode::Local, 1, 1).is_ok());
    assert!(check_owners(Mode::RemoteActive, 1, 2).is_ok());
    assert!(check_owners(Mode::RemoteSleepObserved, 1, 2).is_ok());
    assert!(check_owners(Mode::Local, 1, 2).is_err());
    assert!(check_owners(Mode::RemoteActive, 1, 1).is_err());
    assert!(check_owners(Mode::RemoteSleepObserved, 0, 1).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn all_three_controls_force_exactly_counted_handoffs_and_drain() {
    for mode in [Mode::Local, Mode::RemoteActive, Mode::RemoteSleepObserved] {
        let report = run(Config {
            mode,
            iterations: 16,
            pin: false,
        })
        .unwrap();
        assert_eq!(report.queued_handoffs, 16);
        assert!(report.recipient_parks >= 16);
        assert_eq!(report.samples.len(), 16);
        assert_eq!(
            report.sleep_observations,
            if mode == Mode::RemoteSleepObserved {
                16
            } else {
                0
            }
        );
        assert!(report.maximum() >= report.percentile(50, 100));
    }
}
