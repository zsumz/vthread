use super::{Arc, AtomicU64, Ordering, model, thread};

#[test]
fn adapter_runs_the_shared_threaded_test_body() {
    model(|| {
        let value = Arc::new(AtomicU64::new(0));
        let writer = Arc::clone(&value);
        thread::spawn(move || writer.store(42, Ordering::Release))
            .join()
            .unwrap();
        assert_eq!(value.load(Ordering::Acquire), 42);
    });
}
