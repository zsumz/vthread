//! Per-producer admission evidence that does not serialize unrelated producers.

use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum TestAdmissionPhase {
    Created,
    Producer,
    Reserving,
    Reserved,
    Publishing,
    Published,
    ReserveRejected,
    PublishRejected,
    Done,
}

pub(crate) struct TestAdmissionProgress {
    writer_installed: AtomicBool,
    sequence: AtomicU64,
    index: AtomicUsize,
    phase: AtomicUsize,
    reserve_entries: AtomicUsize,
    reservations: AtomicUsize,
    publish_entries: AtomicUsize,
    publications: AtomicUsize,
    task_capacity: AtomicUsize,
    queue_capacity: AtomicUsize,
    stopped: AtomicUsize,
    other_errors: AtomicUsize,
}

impl Default for TestAdmissionProgress {
    fn default() -> Self {
        Self {
            writer_installed: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
            index: AtomicUsize::new(usize::MAX),
            phase: AtomicUsize::new(0),
            reserve_entries: AtomicUsize::new(0),
            reservations: AtomicUsize::new(0),
            publish_entries: AtomicUsize::new(0),
            publications: AtomicUsize::new(0),
            task_capacity: AtomicUsize::new(0),
            queue_capacity: AtomicUsize::new(0),
            stopped: AtomicUsize::new(0),
            other_errors: AtomicUsize::new(0),
        }
    }
}

impl TestAdmissionProgress {
    pub(crate) fn begin(&self, index: usize) {
        self.update(|| {
            self.index.store(index, Ordering::Relaxed);
            self.phase
                .store(TestAdmissionPhase::Producer as usize, Ordering::Relaxed);
        });
    }

    pub(crate) fn phase(&self, phase: TestAdmissionPhase) {
        self.update(|| {
            let counter = match phase {
                TestAdmissionPhase::Reserving => Some(&self.reserve_entries),
                TestAdmissionPhase::Reserved => Some(&self.reservations),
                TestAdmissionPhase::Publishing => Some(&self.publish_entries),
                TestAdmissionPhase::Published => Some(&self.publications),
                _ => None,
            };
            if let Some(counter) = counter {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            self.phase.store(phase as usize, Ordering::Relaxed);
        });
    }

    pub(crate) fn rejected(&self, phase: TestAdmissionPhase, error: &crate::Error) {
        self.update(|| {
            let counter = match error {
                crate::Error::Capacity {
                    resource: crate::error::CapacityResource::Tasks,
                    ..
                } => &self.task_capacity,
                crate::Error::Capacity {
                    resource: crate::error::CapacityResource::CarrierQueue,
                    ..
                } => &self.queue_capacity,
                crate::Error::RuntimeStopped => &self.stopped,
                _ => &self.other_errors,
            };
            counter.fetch_add(1, Ordering::Relaxed);
            self.phase.store(phase as usize, Ordering::Relaxed);
        });
    }

    pub(crate) fn finish(&self) {
        self.phase(TestAdmissionPhase::Done);
    }

    pub(crate) fn snapshot(&self) -> TestAdmissionProgressSnapshot {
        let before = self.sequence.load(Ordering::Acquire);
        let index = self.index.load(Ordering::Relaxed);
        let mut snapshot = TestAdmissionProgressSnapshot {
            coherent: false,
            sequence: before,
            index: (index != usize::MAX).then_some(index),
            phase: admission_phase(self.phase.load(Ordering::Relaxed)),
            reserve_entries: self.reserve_entries.load(Ordering::Relaxed),
            reservations: self.reservations.load(Ordering::Relaxed),
            publish_entries: self.publish_entries.load(Ordering::Relaxed),
            publications: self.publications.load(Ordering::Relaxed),
            task_capacity: self.task_capacity.load(Ordering::Relaxed),
            queue_capacity: self.queue_capacity.load(Ordering::Relaxed),
            stopped: self.stopped.load(Ordering::Relaxed),
            other_errors: self.other_errors.load(Ordering::Relaxed),
        };
        let after = self.sequence.load(Ordering::Acquire);
        snapshot.coherent = before == after && before.is_multiple_of(2);
        snapshot
    }

    pub(crate) fn assert_successful(&self, tasks: usize) {
        let snapshot = self.snapshot();
        assert!(snapshot.coherent, "incoherent final admission record");
        assert_eq!(snapshot.index, tasks.checked_sub(1));
        assert_eq!(snapshot.phase, TestAdmissionPhase::Done);
        assert_eq!(
            (
                snapshot.reservations,
                snapshot.publish_entries,
                snapshot.publications,
            ),
            (tasks, tasks, tasks)
        );
        assert_eq!(snapshot.reserve_entries, tasks + snapshot.queue_capacity);
        assert_eq!(
            (
                snapshot.task_capacity,
                snapshot.stopped,
                snapshot.other_errors
            ),
            (0, 0, 0)
        );
    }

    fn update(&self, update: impl FnOnce()) {
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel);
        assert!(sequence.is_multiple_of(2), "one writer per producer");
        update();
        self.sequence.store(sequence + 2, Ordering::Release);
    }
}

thread_local! {
    static ADMISSION_PROGRESS: RefCell<Option<Arc<TestAdmissionProgress>>> = const {
        RefCell::new(None)
    };
}

#[must_use = "keep the guard alive while the producer is being observed"]
pub(crate) struct TestAdmissionProgressGuard {
    installed: Arc<TestAdmissionProgress>,
    previous: Option<Arc<TestAdmissionProgress>>,
    not_send: PhantomData<Rc<()>>,
}

impl Drop for TestAdmissionProgressGuard {
    fn drop(&mut self) {
        ADMISSION_PROGRESS.with(|slot| {
            let _installed = slot.replace(self.previous.take());
        });
        self.installed
            .writer_installed
            .store(false, Ordering::Release);
    }
}

pub(crate) fn install_admission_progress(
    progress: Arc<TestAdmissionProgress>,
) -> TestAdmissionProgressGuard {
    assert!(
        progress
            .writer_installed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok(),
        "admission recorder already has a producer"
    );
    let installed = Arc::clone(&progress);
    let previous = ADMISSION_PROGRESS.with(|slot| slot.replace(Some(progress)));
    TestAdmissionProgressGuard {
        installed,
        previous,
        not_send: PhantomData,
    }
}

pub(crate) fn record_admission_phase(phase: TestAdmissionPhase) {
    ADMISSION_PROGRESS.with(|slot| {
        if let Some(progress) = slot.borrow().as_ref() {
            progress.phase(phase);
        }
    });
}

pub(crate) fn record_admission_rejection(phase: TestAdmissionPhase, error: &crate::Error) {
    ADMISSION_PROGRESS.with(|slot| {
        if let Some(progress) = slot.borrow().as_ref() {
            progress.rejected(phase, error);
        }
    });
}

fn admission_phase(value: usize) -> TestAdmissionPhase {
    match value {
        0 => TestAdmissionPhase::Created,
        1 => TestAdmissionPhase::Producer,
        2 => TestAdmissionPhase::Reserving,
        3 => TestAdmissionPhase::Reserved,
        4 => TestAdmissionPhase::Publishing,
        5 => TestAdmissionPhase::Published,
        6 => TestAdmissionPhase::ReserveRejected,
        7 => TestAdmissionPhase::PublishRejected,
        8 => TestAdmissionPhase::Done,
        _ => unreachable!("admission phase"),
    }
}

pub(crate) struct TestAdmissionProgressSnapshot {
    coherent: bool,
    sequence: u64,
    index: Option<usize>,
    phase: TestAdmissionPhase,
    reserve_entries: usize,
    reservations: usize,
    publish_entries: usize,
    publications: usize,
    task_capacity: usize,
    queue_capacity: usize,
    stopped: usize,
    other_errors: usize,
}

impl std::fmt::Debug for TestAdmissionProgressSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdmissionProgress")
            .field("coherent", &self.coherent)
            .field("sequence", &self.sequence)
            .field("index", &self.index)
            .field("phase", &self.phase)
            .field("reserve_entries", &self.reserve_entries)
            .field("reservations", &self.reservations)
            .field("publish_entries", &self.publish_entries)
            .field("publications", &self.publications)
            .field("task_capacity", &self.task_capacity)
            .field("queue_capacity", &self.queue_capacity)
            .field("stopped", &self.stopped)
            .field("other_errors", &self.other_errors)
            .finish()
    }
}

#[test]
fn per_producer_recorders_restore_nested_thread_local_state() {
    let outer = Arc::new(TestAdmissionProgress::default());
    let inner = Arc::new(TestAdmissionProgress::default());
    {
        let _outer = install_admission_progress(Arc::clone(&outer));
        outer.begin(7);
        record_admission_phase(TestAdmissionPhase::Reserving);
        {
            let _inner = install_admission_progress(Arc::clone(&inner));
            inner.begin(3);
            record_admission_phase(TestAdmissionPhase::Publishing);
        }
        record_admission_phase(TestAdmissionPhase::Published);
    }
    record_admission_phase(TestAdmissionPhase::Done);
    let outer = outer.snapshot();
    assert_eq!(outer.index, Some(7));
    assert_eq!(outer.phase, TestAdmissionPhase::Published);
    assert_eq!((outer.reserve_entries, outer.publications), (1, 1));
    let inner = inner.snapshot();
    assert_eq!(inner.index, Some(3));
    assert_eq!(inner.phase, TestAdmissionPhase::Publishing);
    assert_eq!(inner.publish_entries, 1);
}
