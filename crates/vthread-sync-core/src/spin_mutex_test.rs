use std::sync::Arc;

use super::SpinMutex;

#[test]
fn guards_exclude_and_preserve_mutation() {
    let mutex = SpinMutex::new(0);
    *mutex.lock() = 42;
    assert_eq!(*mutex.lock(), 42);
}

#[test]
fn contention_preserves_every_update() {
    let mutex = Arc::new(SpinMutex::new(0));
    let threads = (0..4)
        .map(|_| {
            let mutex = Arc::clone(&mutex);
            std::thread::spawn(move || {
                for _ in 0..10_000 {
                    *mutex.lock() += 1;
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(*mutex.lock(), 40_000);
}

#[test]
fn panic_releases_without_poisoning() {
    let mutex = SpinMutex::new(0);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = mutex.lock();
        panic!("injected panic");
    }));
    assert!(result.is_err());
    *mutex.lock() = 42;
    assert_eq!(*mutex.lock(), 42);
}
