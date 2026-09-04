use super::WaitShard;
use crate::{cancellation::WaitSlot, wait::WaitCell};
use vthread_stack::ParkToken;

fn slot(identity: u64, generation: u64) -> WaitSlot {
    WaitSlot {
        token: ParkToken::new(identity, generation),
        registration: WaitCell::new().registration(),
    }
}

#[test]
fn one_entry_stays_inline_and_reuses_the_empty_slot() {
    let mut waits = WaitShard::default();
    assert!(waits.try_insert(1, slot(10, 1)));
    assert_eq!(waits.get(1).unwrap().token, ParkToken::new(10, 1));
    waits.remove(1, ParkToken::new(10, 1));
    assert!(waits.get(1).is_none());
    assert!(waits.try_insert(2, slot(20, 1)));
    assert_eq!(waits.get(2).unwrap().token, ParkToken::new(20, 1));
}

#[test]
fn duplicate_node_rejection_preserves_the_registered_generation() {
    let mut waits = WaitShard::default();
    assert!(waits.try_insert(1, slot(10, 1)));
    assert!(!waits.try_insert(1, slot(10, 2)));
    assert_eq!(waits.get(1).unwrap().token, ParkToken::new(10, 1));
    waits.remove(1, ParkToken::new(10, 2));
    assert_eq!(waits.get(1).unwrap().token, ParkToken::new(10, 1));
}

#[test]
fn colliding_nodes_promote_and_remove_independently() {
    let mut waits = WaitShard::default();
    assert!(waits.try_insert(1, slot(10, 1)));
    assert!(waits.try_insert(65, slot(20, 1)));
    assert_eq!(waits.get(1).unwrap().token, ParkToken::new(10, 1));
    assert_eq!(waits.get(65).unwrap().token, ParkToken::new(20, 1));
    waits.remove(1, ParkToken::new(10, 1));
    assert!(waits.get(1).is_none());
    assert_eq!(waits.get(65).unwrap().token, ParkToken::new(20, 1));
    waits.remove(65, ParkToken::new(20, 1));
    assert!(waits.get(65).is_none());
}
