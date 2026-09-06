use super::{Handoff, Ordering, Phase, Publication, WakeCause, model};
use loom::{sync::Arc, thread};

#[test]
fn completion_registration_and_owner_sleep_cannot_lose_a_wake() {
    for resource in [false, true] {
        model(move || {
            let handoff = Arc::new(Handoff::new());
            let sender = Arc::clone(&handoff);
            let publisher = thread::spawn(move || {
                let claimed = sender.claim(WakeCause::Ready, resource).unwrap();
                sender.route.push(41);
                sender.publish(claimed, true);
            });
            handoff.drive(false);
            publisher.join().unwrap();
            assert_eq!(handoff.load().phase(), Phase::Idle);
            assert_eq!(handoff.consumed.load(Ordering::Relaxed), 1);
            assert_eq!(handoff.ownership.load(Ordering::Relaxed), 0);
            assert_eq!(handoff.publication(41), Publication::Stale);
            assert!(handoff.route.pop().is_none());
        });
    }
}

#[test]
fn abandonment_keeps_selected_ownership_until_publication_finishes() {
    model(|| {
        let handoff = Arc::new(Handoff::new());
        let sender = Arc::clone(&handoff);
        let publisher = thread::spawn(move || {
            if let Some(claimed) = sender.claim(WakeCause::Ready, true) {
                sender.route.push(41);
                sender.publish(claimed, true);
                true
            } else {
                false
            }
        });
        handoff.drive(true);
        let selected = publisher.join().unwrap();
        assert_eq!(handoff.ownership.load(Ordering::Relaxed), 0);
        assert_eq!(
            handoff.recovered.load(Ordering::Relaxed),
            usize::from(selected)
        );
        assert_eq!(handoff.consumed.load(Ordering::Relaxed), 0);
        assert_eq!(handoff.load().phase(), Phase::Idle);
        // A queued notice may still exist after abandonment, but retirement was
        // already legal and the owner rejects it before route reuse.
        if let Some(generation) = handoff.route.pop() {
            assert_eq!(handoff.publication(generation), Publication::Stale);
        }
    });
}

#[test]
fn an_incomplete_claim_does_not_authorize_mount_or_ancestor_reclamation() {
    model(|| {
        let handoff = Handoff::new();
        let claimed = handoff.claim(WakeCause::Ready, true).unwrap();
        handoff.route.push(41);
        assert_eq!(handoff.route.pop(), Some(41));
        assert_eq!(handoff.publication(41), Publication::InFlight);
        assert!(!handoff.try_abandon(41));
        assert_eq!(handoff.ownership.load(Ordering::Relaxed), 1);
        // Owner has returned to scheduling. Its exact parked record and the
        // ancestor environment remain live; unrelated work may run here.
        assert_eq!(handoff.consumed.load(Ordering::Relaxed), 0);
        handoff.publish(claimed, true);
        assert!(handoff.try_abandon(41));
        assert_eq!(handoff.recovered.load(Ordering::Relaxed), 1);
        assert!(handoff.try_abandon(41));
        assert_eq!(handoff.recovered.load(Ordering::Relaxed), 1);
    });
}

#[test]
#[should_panic(expected = "deadlock")]
fn omitting_completion_notification_has_a_sleeping_owner_counterexample() {
    model(|| {
        // Loom's deadlock panic has no active modeled thread for Arc::drop.
        // Keep that secondary destructor panic from hiding the negative control;
        // successful executions still drop normally and run Loom's leak checks.
        let handoff = std::mem::ManuallyDrop::new(Arc::new(Handoff::new()));
        let sender = Arc::clone(&handoff);
        let publisher = thread::spawn(move || {
            let claimed = sender.claim(WakeCause::Ready, false).unwrap();
            sender.route.push(41);
            sender.publish(claimed, false);
        });
        handoff.drive(false);
        publisher.join().unwrap();
        drop(std::mem::ManuallyDrop::into_inner(handoff));
    });
}

#[test]
fn a_held_publisher_allows_unrelated_work_but_not_selected_resource_reclamation() {
    model(|| {
        let handoff = Arc::new(Handoff::new());
        let sender = Arc::clone(&handoff);
        let (published, observe) = loom::sync::mpsc::channel();
        let (resume, held) = loom::sync::mpsc::channel();
        let publisher = thread::spawn(move || {
            let claimed = sender.claim(WakeCause::Ready, true).unwrap();
            sender.route.push(41);
            published.send(()).unwrap();
            held.recv().unwrap();
            sender.publish(claimed, true);
        });
        observe.recv().unwrap();
        assert_eq!(handoff.route.pop(), Some(41));
        assert_eq!(handoff.publication(41), Publication::InFlight);
        assert!(!handoff.try_abandon(41));
        let unrelated = loom::sync::atomic::AtomicUsize::new(0);
        unrelated.fetch_add(1, Ordering::Relaxed);
        assert_eq!(unrelated.load(Ordering::Relaxed), 1);
        assert_eq!(handoff.ownership.load(Ordering::Relaxed), 1);
        assert!(handoff.load().is_claimed());
        resume.send(()).unwrap();
        publisher.join().unwrap();
        handoff.consume(41);
    });
}

#[test]
fn ready_competing_with_other_causes_produces_one_owned_resume() {
    for cause in [
        WakeCause::TimedOut,
        WakeCause::Cancelled,
        WakeCause::InheritedCancelled,
        WakeCause::Closed,
    ] {
        model(move || {
            let handoff = Arc::new(Handoff::new());
            let ready = Arc::clone(&handoff);
            let publisher = thread::spawn(move || {
                if let Some(claimed) = ready.claim(WakeCause::Ready, true) {
                    ready.route.push(41);
                    ready.publish(claimed, true);
                }
            });
            let other = Arc::clone(&handoff);
            let competitor = thread::spawn(move || {
                if let Some(claimed) = other.claim(cause, false) {
                    other.route.push(41);
                    other.publish(claimed, true);
                }
            });
            // Explore cause competition with an active owner. The separate
            // completion/sleep test explores the winner with a sleeping owner:
            // the losing selector performs no route, ownership or signal writes
            // after its failed claim CAS. Do not multiply that inert actor into
            // every condition-variable schedule or silently cap permutations.
            let observed = handoff.route.pop();
            if let Some(generation) = observed {
                assert_ne!(handoff.publication(generation), Publication::Stale);
            }
            publisher.join().unwrap();
            competitor.join().unwrap();
            let generation = observed.or_else(|| handoff.route.pop()).unwrap();
            assert_eq!(handoff.publication(generation), Publication::Published);
            handoff.consume(generation);
            assert_eq!(handoff.consumed.load(Ordering::Relaxed), 1);
            assert_eq!(handoff.recovered.load(Ordering::Relaxed), 0);
            assert_eq!(handoff.ownership.load(Ordering::Relaxed), 0);
            assert!(handoff.route.pop().is_none());
        });
    }
}

#[test]
fn delayed_completion_signal_does_not_touch_a_reused_generation() {
    model(|| {
        let handoff = Arc::new(Handoff::new());
        let claimed = handoff.claim(WakeCause::Ready, false).unwrap();
        handoff.route.push(41);
        assert_eq!(handoff.route.pop(), Some(41));
        assert_eq!(handoff.publication(41), Publication::InFlight);
        // Stop after selected publication, before the independent completion
        // signal. The old publisher must have no subsequent route/state writes.
        let watched = handoff.load();
        assert_eq!(watched.with_permit(false), claimed);
        assert!(handoff.replace(watched, watched.publish_claim().with_permit(false)));
        let old = Arc::clone(&handoff);
        let delayed = thread::spawn(move || old.route.signal.notify());
        handoff.consume(41);
        assert!(handoff.replace(handoff.load(), Handoff::active(42)));
        assert_eq!(handoff.publication(41), Publication::Stale);
        let next = handoff.claim(WakeCause::Ready, true).unwrap();
        handoff.route.push(42);
        handoff.publish(next, true);
        assert_eq!(handoff.route.pop(), Some(42));
        handoff.consume(42);
        delayed.join().unwrap();
        assert_eq!(handoff.load().generation(), 42);
        assert_eq!(handoff.consumed.load(Ordering::Relaxed), 2);
        assert_eq!(handoff.ownership.load(Ordering::Relaxed), 0);
        assert!(handoff.route.pop().is_none());
    });
}

#[test]
#[should_panic(expected = "never mount an incomplete claim")]
fn mounting_before_publication_is_rejected() {
    model(|| {
        let handoff = Handoff::new();
        handoff.claim(WakeCause::Ready, false).unwrap();
        handoff.route.push(41);
        handoff.consume(handoff.route.pop().unwrap());
    });
}
