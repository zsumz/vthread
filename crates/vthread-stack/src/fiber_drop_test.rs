use std::{cell::Cell, ptr, rc::Rc};

use crate::{Fiber, FiberState, MappedStack, Suspension, context::FiberCore, mount, suspend};

struct ObserveMount(Rc<Cell<*const FiberCore>>);

impl Drop for ObserveMount {
    fn drop(&mut self) {
        self.0.set(mount::mounted_core());
    }
}

fn fiber(body: impl FnOnce() + 'static) -> Fiber {
    Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), body)
}

#[test]
fn dropping_an_unstarted_entry_does_not_mount_its_never_saved_context() {
    let observed = Rc::new(Cell::new(ptr::null()));
    let capture = ObserveMount(Rc::clone(&observed));
    drop(fiber(move || drop(capture)));
    assert!(observed.get().is_null(), "unstarted entry installed a core");
    assert!(mount::mounted_core().is_null());
}

#[test]
fn an_unstarted_entry_destructor_keeps_the_actual_outer_fiber_mounted() {
    let observed = Rc::new(Cell::new(ptr::null()));
    let body_observed = Rc::clone(&observed);
    let mut outer = fiber(move || {
        let actual = mount::mounted_core();
        assert!(!actual.is_null());
        let capture = ObserveMount(body_observed.clone());
        drop(fiber(move || drop(capture)));
        assert_eq!(body_observed.get(), actual);
        assert_eq!(mount::mounted_core(), actual);
        suspend(Suspension::YieldNow).unwrap();
    });
    assert_eq!(outer.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(outer.resume(), FiberState::Complete);
    assert!(!observed.get().is_null());
    assert!(mount::mounted_core().is_null());
}

#[test]
fn unstarted_entry_destructors_suspend_only_an_actual_running_fiber() {
    const PROBE: &str = "VTHREAD_UNSTARTED_DROP_PROBE";
    if std::env::var_os(PROBE).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fiber::fiber_drop_test::unstarted_entry_destructors_suspend_only_an_actual_running_fiber",
                "--nocapture",
            ])
            .env(PROBE, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "destructor suspension probe failed: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    struct RejectSuspend(Rc<Cell<bool>>);
    impl Drop for RejectSuspend {
        fn drop(&mut self) {
            self.0.set(suspend(Suspension::YieldNow).is_err());
        }
    }
    let rejected = Rc::new(Cell::new(false));
    let capture = RejectSuspend(Rc::clone(&rejected));
    drop(fiber(move || drop(capture)));
    assert!(
        rejected.get(),
        "carrier destructor attempted a stack switch"
    );

    struct YieldOnDrop;
    impl Drop for YieldOnDrop {
        fn drop(&mut self) {
            suspend(Suspension::YieldNow).unwrap();
        }
    }
    let completed = Rc::new(Cell::new(false));
    let body_completed = Rc::clone(&completed);
    let mut outer = fiber(move || {
        let capture = YieldOnDrop;
        drop(fiber(move || drop(capture)));
        body_completed.set(true);
    });
    assert_eq!(outer.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert!(!completed.get());
    assert_eq!(outer.resume(), FiberState::Complete);
    assert!(completed.get());
    assert!(mount::mounted_core().is_null());
}
