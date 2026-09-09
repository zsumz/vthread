use std::{sync::Arc, thread};

use super::{
    NotifyResult, ResourceSelection, WaitBegin, WaitCell, WaitHub, WakeCause,
    wait_publication_probe_test::{PausedPublication, Stage},
    wait_state::Phase,
};
use crate::{TaskId, task_slab::TaskKey};
use vthread_stack::ParkToken;

#[derive(Clone, Copy, Debug)]
enum Selection {
    Ready,
    Permit,
    Timeout,
    DirectCancel,
    InheritedCancel,
    Close,
}

const SELECTIONS: [Selection; 6] = [
    Selection::Ready,
    Selection::Permit,
    Selection::Timeout,
    Selection::DirectCancel,
    Selection::InheritedCancel,
    Selection::Close,
];

impl Selection {
    fn select(self, cell: &WaitCell, token: ParkToken) -> WakeCause {
        match self {
            Self::Ready => assert_eq!(cell.notify(), NotifyResult::Woke),
            Self::Permit => cell
                .clone()
                .reserve_resource(ResourceSelection::Permit)
                .unwrap()
                .publish(),
            Self::Timeout => assert!(cell.select_timeout(token).unwrap()),
            Self::DirectCancel => assert!(cell.cancel()),
            Self::InheritedCancel => assert!(cell.registration().select_cancelled(token)),
            Self::Close => assert!(cell.close()),
        }
        self.cause()
    }

    fn cause(self) -> WakeCause {
        match self {
            Self::Ready | Self::Permit => WakeCause::Ready,
            Self::Timeout => WakeCause::TimedOut,
            Self::DirectCancel => WakeCause::Cancelled,
            Self::InheritedCancel => WakeCause::InheritedCancelled,
            Self::Close => WakeCause::Closed,
        }
    }
}

fn begin(cell: &WaitCell, hub: &Arc<WaitHub>, task: u64) -> ParkToken {
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(task), TaskKey::owned(0), hub, None)
        .unwrap()
    else {
        panic!("expected a fresh park");
    };
    request.token()
}

#[test]
fn visible_notices_keep_all_six_winners_claimed_until_the_publisher_finishes() {
    for selection in SELECTIONS {
        let cell = WaitCell::new();
        let hub = Arc::new(WaitHub::new(1, Arc::default()));
        let token = begin(&cell, &hub, 1);
        thread::scope(|threads| {
            let mut pause = PausedPublication::install(&cell);
            let publisher = threads.spawn(|| selection.select(&cell, token));
            let published = pause.observe(Stage::NoticePublished);
            assert_eq!(published.token, token);
            assert!(cell.state.load().is_claimed(), "{selection:?}");
            let notice = hub.pop_wake().expect("notice already consumable");
            assert_eq!(notice.token, token);
            assert_eq!(notice.cause, selection.cause());
            let registration = cell.registration();
            assert!(!registration.select_ready(token));
            assert!(!registration.select_timeout(token).unwrap());
            assert!(!registration.select_cancelled(token));
            assert!(!registration.select_closed(token));
            assert!(!cell.offer_resource(ResourceSelection::Permit));
            let recipient = threads.spawn(|| {
                if matches!(selection, Selection::Permit) {
                    cell.finish_permit_ready(token)
                } else {
                    cell.finish_plain_ready(token)
                }
            });
            let waiting = pause.observe(Stage::FinishWaiting);
            assert_eq!(waiting.token, token);
            assert_ne!(published.thread, waiting.thread);
            assert!(waiting.elapsed >= published.elapsed);
            pause.observe(Stage::FinishSpinning);
            assert!(cell.state.load().is_claimed());
            assert!(hub.pop_wake().is_none());
            pause.release();
            assert_eq!(publisher.join().unwrap(), selection.cause());
            assert_eq!(recipient.join().unwrap().unwrap(), selection.cause());
        });
        assert_eq!(cell.state.load().phase(), Phase::Idle);
        assert_eq!(cell.take_resource(), None);
        assert!(hub.pop_wake().is_none());
    }
}

#[test]
fn retirement_waits_for_publication_and_stale_events_cannot_reach_reuse() {
    for selection in SELECTIONS {
        let cell = WaitCell::new();
        let hub = Arc::new(WaitHub::new(1, Arc::default()));
        let token = begin(&cell, &hub, 1);
        let registration = cell.registration();
        thread::scope(|threads| {
            let mut pause = PausedPublication::install(&cell);
            let publisher = threads.spawn(|| selection.select(&cell, token));
            pause.observe(Stage::NoticePublished);
            assert_eq!(hub.pending(), 1);
            let cleanup = threads.spawn(|| registration.abandon(token));
            pause.observe(Stage::RetireWaiting);
            assert!(cell.state.load().is_claimed());
            assert_eq!(hub.pending(), 1);
            pause.release();
            publisher.join().unwrap();
            cleanup.join().unwrap();
        });
        assert!(hub.pop_wake().is_none());
        assert_eq!(cell.state.load().phase(), Phase::Idle);
        let expected = matches!(selection, Selection::Permit).then_some(ResourceSelection::Permit);
        assert_eq!(cell.take_resource(), expected);
        assert_eq!(
            cell.take_resource(),
            None,
            "selected resource recovered once"
        );
        if matches!(selection, Selection::Close) {
            // Only a gate with an exclusive retired ticket can reset close.
            assert!(cell.reset_closed_gate());
        }
        let next = begin(&cell, &hub, 2);
        assert!(next.generation() > token.generation());
        assert!(!registration.select_ready(token));
        assert!(!registration.select_timeout(token).unwrap());
        assert!(!registration.select_cancelled(token));
        assert!(!registration.select_closed(token));
        assert!(hub.pop_wake().is_none());
        assert!(cell.registration().select_ready(next));
        let notice = hub.pop_wake().unwrap();
        assert_eq!(notice.token, next);
        assert_eq!(notice.task, TaskId::new(2));
        assert_eq!(cell.finish(next).unwrap(), WakeCause::Ready);
        assert_eq!(hub.stale(), 0);
    }
}

#[test]
fn unwinding_the_observer_releases_a_paused_publisher() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let token = begin(&cell, &hub, 1);
    thread::scope(|threads| {
        let pause = PausedPublication::install(&cell);
        let publisher = threads.spawn(|| cell.notify());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let pause = pause;
                pause.observe(Stage::NoticePublished);
                panic!("observer failed while publisher was paused");
            }))
            .is_err()
        );
        assert_eq!(publisher.join().unwrap(), NotifyResult::Woke);
    });
    assert_eq!(hub.pop_wake().unwrap().token, token);
    assert_eq!(cell.finish(token).unwrap(), WakeCause::Ready);
}

#[test]
fn a_registered_native_waiter_receives_the_notice_before_claim_completion() {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    let cell = WaitCell::new();
    let signal = Arc::default();
    let hub = Arc::new(WaitHub::new(1, Arc::clone(&signal)));
    let token = begin(&cell, &hub, 1);
    let epoch = signal.version();
    thread::scope(|threads| {
        let mut pause = PausedPublication::install(&cell);
        let (dequeued_tx, dequeued_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let wait_hub = Arc::clone(&hub);
        let wait_cell = &cell;
        let recipient = threads.spawn(move || {
            wait_hub.wait_while(epoch, Some(Instant::now() + Duration::from_secs(5)), || {
                false
            });
            let notice = wait_hub.pop_wake().expect("published wake");
            let _ = dequeued_tx.send(notice);
            let _ = finish_rx.recv();
            wait_cell.finish(token).unwrap()
        });
        crate::support_test::until(|| signal.waiting() == 1);
        let publisher = threads.spawn(|| cell.notify());
        pause.observe(Stage::NoticePublished);
        let notice = dequeued_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(notice.token, token);
        assert_eq!(notice.cause, WakeCause::Ready);
        assert_eq!(signal.version(), epoch, "predicate wake needs no new epoch");
        finish_tx.send(()).unwrap();
        pause.observe(Stage::FinishWaiting);
        pause.observe(Stage::FinishSpinning);
        assert!(cell.state.load().is_claimed());
        pause.release();
        assert_eq!(publisher.join().unwrap(), NotifyResult::Woke);
        assert_eq!(recipient.join().unwrap(), WakeCause::Ready);
    });
    assert_eq!(signal.waiting(), 0);
    assert!(hub.pop_wake().is_none());
}
