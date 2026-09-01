//! Collision-safe equality for canonical Patricia signatures.

use super::{Counter, Node, NodeKind};
use std::sync::Arc;

pub(super) fn same(
    left: Option<&Arc<Node>>,
    right: Option<&Arc<Node>>,
    counter: &mut Counter,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => same_node(left, right, counter),
        (None, None) => true,
        _ => false,
    }
}

fn same_node(left: &Arc<Node>, right: &Arc<Node>, counter: &mut Counter) -> bool {
    counter.equality_node();
    if Arc::ptr_eq(left, right) {
        return true;
    }
    if left.len != right.len || left.xor != right.xor || left.sum != right.sum {
        return false;
    }
    match (&left.kind, &right.kind) {
        (NodeKind::Leaf(left), NodeKind::Leaf(right)) => left == right,
        (
            NodeKind::Branch {
                bit: left_bit,
                left: left_zero,
                right: left_one,
            },
            NodeKind::Branch {
                bit: right_bit,
                left: right_zero,
                right: right_one,
            },
        ) => {
            left_bit == right_bit
                && same_node(left_zero, right_zero, counter)
                && same_node(left_one, right_one, counter)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "cancellation_signature_compare_test.rs"]
mod cancellation_signature_compare_test;
