use super::{ParkedTask, ParkedTasks};
use crate::{task_slab::TaskKey, wait::WaitCell};
use vthread_stack::ParkToken;

fn parked(task: TaskKey, token: ParkToken) -> ParkedTask {
    ParkedTask {
        token,
        task,
        has_deadline: false,
        registration: Some(WaitCell::new().registration()),
    }
}

#[test]
fn owned_and_borrowed_routes_are_independent_and_exact() {
    let owned = TaskKey::owned(3);
    let borrowed = TaskKey::borrowed(3);
    let owned_token = ParkToken::new(7, 1);
    let borrowed_token = ParkToken::new(8, 1);
    let mut tasks = ParkedTasks::new();

    assert!(tasks.insert(parked(owned, owned_token)));
    assert!(tasks.insert(parked(borrowed, borrowed_token)));
    assert!(!tasks.insert(parked(owned, ParkToken::new(9, 1))));
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks.get(owned).expect("owned route").token, owned_token);
    assert_eq!(
        tasks.get(borrowed).expect("borrowed route").token,
        borrowed_token
    );
    assert_eq!(tasks.get(borrowed).expect("borrowed route").task, borrowed);

    assert_eq!(
        tasks.remove(owned).expect("remove owned").token,
        owned_token
    );
    assert!(tasks.get(owned).is_none());
    assert_eq!(tasks.len(), 1);
    assert!(!tasks.is_empty());
}
