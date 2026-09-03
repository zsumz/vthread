use super::{Node, NodeKind, Root};
use std::sync::Arc;

pub(super) struct Iter<'a> {
    singleton: Option<usize>,
    stack: Vec<&'a Arc<Node>>,
}

impl<'a> Iter<'a> {
    pub(super) fn new(root: &'a Root) -> Self {
        match root {
            Root::Empty => Self {
                singleton: None,
                stack: Vec::new(),
            },
            Root::Singleton(id) => Self {
                singleton: Some(*id),
                stack: Vec::new(),
            },
            Root::Tree(root) => Self {
                singleton: None,
                stack: vec![root],
            },
        }
    }
}

impl Iterator for Iter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(id) = self.singleton.take() {
            return Some(id);
        }
        while let Some(node) = self.stack.pop() {
            match &node.kind {
                NodeKind::Leaf(id) => return Some(*id),
                NodeKind::Branch { left, right, .. } => {
                    self.stack.push(right);
                    self.stack.push(left);
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "cancellation_signature_iter_test.rs"]
mod cancellation_signature_iter_test;
