use super::CancellationToken;
use std::{
    sync::{Arc, Barrier},
    thread,
};

#[test]
fn child_registration_and_intermediate_pruning_race_cancellation() {
    for cancel_owner in [false, true] {
        for _ in 0..128 {
            let ancestor = CancellationToken::root(2);
            let owner = ancestor.child_token();
            let parent = ancestor.child_token();
            let retained = parent.clone();
            let barrier = Arc::new(Barrier::new(2));
            let remote_barrier = Arc::clone(&barrier);
            let remote_owner = owner.clone();
            let worker = thread::spawn(move || {
                remote_barrier.wait();
                let intermediate = parent.child_for_scope(&remote_owner);
                let descendant = intermediate.child_token();
                drop(intermediate);
                descendant
            });
            barrier.wait();
            if cancel_owner {
                owner.cancel();
            } else {
                retained.cancel();
            }
            let descendant = worker.join().unwrap();
            assert!(descendant.is_cancelled());
            assert!(!ancestor.is_cancelled());
            assert_eq!(retained.is_cancelled(), !cancel_owner);
            assert_eq!(owner.is_cancelled(), cancel_owner);
        }
    }
}

#[test]
fn dropping_a_retained_deep_token_chain_does_not_recurse() {
    thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let root = CancellationToken::root(2);
            let mut tokens = vec![root.clone()];
            for _ in 0..100_000 {
                tokens.push(tokens.last().unwrap().child_token());
            }
            let last = tokens.last().unwrap().clone();
            drop(tokens);
            assert_eq!(root.graph_snapshot(), (2, 1));
            root.cancel();
            assert!(last.is_cancelled());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn alternating_owners_prune_history_without_losing_any_retained_canceller() {
    for canceller in 0..3 {
        let root = CancellationToken::root(2);
        let owners = [root.child_token(), root.child_token()];
        let ancestor = owners[0].child_token();
        let mut current = ancestor.child_for_scope(&owners[1]);
        for generation in 0..100_000 {
            current = current.child_for_scope(&owners[generation % 2]);
            if generation % 64 == 0 {
                let (nodes, edges) = current.graph_snapshot();
                assert_eq!(nodes, 5);
                assert!(edges <= 6);
            }
        }
        [&ancestor, &owners[0], &owners[1]][canceller].cancel();
        assert!(current.is_cancelled());
        assert!(!root.is_cancelled());
        assert!(!owners[usize::from(canceller != 2)].is_cancelled());
    }
}
