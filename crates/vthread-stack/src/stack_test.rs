use std::io::ErrorKind;

use super::{MappedStack, STACK_ALIGNMENT};
use crate::stack_unix::page_size;

fn stack(usable: usize) -> MappedStack {
    MappedStack::new(usable, 7).expect("allocate stack")
}

#[test]
fn capacity_rounds_up_to_whole_pages_above_one_guard_page() {
    let page = page_size().unwrap();
    let stack = stack(1);
    assert_eq!(stack.usable_len(), page);
    assert_eq!(stack.guard_len(), page);
    assert_eq!(stack.base().get() - stack.limit().get(), 2 * page);
    assert_eq!(stack.base().get() % STACK_ALIGNMENT, 0);
    assert_eq!(stack.limit().get() % page, 0);
    assert_eq!(stack.identity(), 7);
    let larger = super::MappedStack::new(page + 1, 8).unwrap();
    assert_eq!(larger.usable_len(), 2 * page);
    assert_eq!(larger.identity(), 8);
}

#[test]
fn the_whole_usable_range_is_writable() {
    let stack = stack(64 * 1024);
    let lowest = (stack.limit().get() + stack.guard_len()) as *mut u8;
    let highest = (stack.base().get() - 1) as *mut u8;
    // SAFETY: both addresses lie inside the enabled range of a mapping this test owns.
    unsafe {
        lowest.write_volatile(1);
        highest.write_volatile(2);
        assert_eq!(lowest.read_volatile(), 1);
        assert_eq!(highest.read_volatile(), 2);
    }
}

#[test]
fn empty_and_overflowing_capacities_are_rejected() {
    assert_eq!(
        MappedStack::new(0, 1).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        MappedStack::new(usize::MAX, 1).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        MappedStack::new(usize::MAX - 4096, 1).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
}

#[test]
fn stacks_may_move_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MappedStack>();
}

#[test]
fn touching_the_guard_page_stops_the_process() {
    const CHILD: &str = "VTHREAD_GUARD_PAGE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::process::ExitStatusExt;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "stack::stack_test::touching_the_guard_page_stops_the_process",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(
            matches!(status.signal(), Some(libc::SIGSEGV | libc::SIGBUS)),
            "guard page access must fault the child: {status:?}"
        );
        return;
    }
    let stack = stack(64 * 1024);
    let guard = (stack.base().get() - stack.usable_len() - 1) as *mut u8;
    // SAFETY: this deliberately faults on the inaccessible guard page in a child process.
    unsafe { guard.write_volatile(0) };
    panic!("guard page write must not succeed");
}
