//! The actual MPSC slot/list algorithm composed with two exact wait identities.

use super::{Handoff, Ordering, Publication, Route, TaskKey, WakeCause, WakeNotice, model};
use loom::{sync::Arc, thread};
use std::io::Write;

fn identity(notice: WakeNotice) -> usize {
    assert!((1..=2).contains(&notice.task.get()));
    assert_eq!(notice.token.wait(), notice.task.get());
    let index = notice.task.get() as usize - 1;
    assert_eq!(notice.route, TaskKey::owned(index));
    assert_eq!(notice.cause, WakeCause::Ready);
    index
}

#[test]
fn two_publishers_preserve_exact_routes_during_owner_deferral() {
    let schedules = model(|| {
        let hub = Arc::new(Route::new());
        let records = Arc::new([
            Handoff::with_route(Arc::clone(&hub), 1),
            Handoff::with_route(Arc::clone(&hub), 2),
        ]);
        let producers = (0..2)
            .map(|index| {
                let records = Arc::clone(&records);
                thread::spawn(move || {
                    let task = &records[index];
                    let claimed = task.claim(WakeCause::Ready, true).unwrap();
                    task.route_claim(41);
                    task.publish(claimed, true);
                })
            })
            .collect::<Vec<_>>();
        let mut deferred = [false; 2];
        if let Some(notice) = hub.queue.pop() {
            let index = identity(notice);
            assert_ne!(records[index].published(41), Publication::Stale);
            deferred[index] = true;
        }
        for producer in producers {
            producer.join().unwrap();
        }
        while let Some(notice) = hub.queue.pop() {
            let index = identity(notice);
            assert!(!deferred[index], "route made ready twice");
            deferred[index] = true;
        }
        assert_eq!(deferred, [true, true]);
        for task in records.iter() {
            assert_eq!(task.published(41), Publication::Published);
            task.consume(41);
            assert_eq!(task.consumed.load(Ordering::Relaxed), 1);
            assert_eq!(task.ownership.load(Ordering::Relaxed), 0);
        }
        assert!(!hub.queue.has_pending());
        assert_eq!(hub.queue.pending(), 0);
    });
    assert!(
        schedules > 1,
        "concurrent routing did not explore alternative schedules"
    );
    writeln!(
        std::io::stderr().lock(),
        "two-route publication completed {schedules} modeled schedules"
    )
    .unwrap();
}

#[test]
fn a_retired_route_can_reenter_while_an_older_consumer_batch_remains() {
    let schedules = model(|| {
        let hub = Arc::new(Route::new());
        let records = Arc::new([
            Handoff::with_route(Arc::clone(&hub), 1),
            Handoff::with_route(Arc::clone(&hub), 2),
        ]);
        for task in records.iter() {
            let claimed = task.claim(WakeCause::Ready, false).unwrap();
            task.route_claim(41);
            task.publish(claimed, true);
        }
        assert_eq!(identity(hub.queue.pop().unwrap()), 0);
        records[0].consume(41);
        assert!(records[0].replace(records[0].load(), Handoff::active(42)));
        let next = Arc::clone(&records);
        let publisher = thread::spawn(move || {
            let claimed = next[0].claim(WakeCause::Ready, true).unwrap();
            next[0].route_claim(42);
            thread::yield_now();
            next[0].publish(claimed, true);
        });
        thread::yield_now();
        assert_eq!(identity(hub.queue.pop().unwrap()), 1);
        records[1].consume(41);
        // Do not join away the publication race: after consuming the retained
        // older batch, probe the republished route while its producer is live.
        let early = hub.queue.pop();
        if let Some(notice) = early {
            assert_eq!(identity(notice), 0);
            assert_ne!(records[0].published(42), Publication::Stale);
        }
        publisher.join().unwrap();
        assert_eq!(records[0].published(41), Publication::Stale);
        let notice = early.or_else(|| hub.queue.pop()).unwrap();
        assert_eq!(identity(notice), 0);
        assert_eq!(notice.token.generation(), 42);
        records[0].consume(42);
        assert!(!hub.queue.has_pending());
        assert_eq!(hub.queue.pending(), 0);
        assert_eq!(records[0].consumed.load(Ordering::Relaxed), 2);
    });
    assert!(
        schedules > 1,
        "concurrent reuse did not explore alternative schedules"
    );
    writeln!(
        std::io::stderr().lock(),
        "route reuse completed {schedules} modeled schedules"
    )
    .unwrap();
}
