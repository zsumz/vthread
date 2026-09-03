//! Exact, structurally shared live-ancestry signatures.

use std::sync::Arc;

#[path = "cancellation_signature_compare.rs"]
mod compare;
#[path = "cancellation_signature_work.rs"]
mod work;
use work::Counter;

#[path = "cancellation_signature_iter.rs"]
mod iter;
use iter::Iter;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Work {
    pub(super) union_items: usize,
    pub(super) equality_nodes: usize,
    pub(super) allocated_nodes: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Candidate {
    len: usize,
    xor: u64,
    sum: u64,
}

#[derive(Clone, Default)]
pub(super) struct Signature(Root);

#[derive(Clone, Default)]
enum Root {
    #[default]
    Empty,
    Singleton(usize),
    Tree(Arc<Node>),
}

struct Node {
    kind: NodeKind,
    sample: usize,
    len: usize,
    xor: u64,
    sum: u64,
}

enum NodeKind {
    Leaf(usize),
    Branch {
        bit: u32,
        left: Arc<Node>,
        right: Arc<Node>,
    },
}

impl Signature {
    pub(super) fn singleton(id: usize) -> Self {
        Self(Root::Singleton(id))
    }

    pub(super) fn cardinality(&self) -> usize {
        match &self.0 {
            Root::Empty => 0,
            Root::Singleton(_) => 1,
            Root::Tree(node) => node.len,
        }
    }

    pub(super) fn candidate(&self) -> Candidate {
        match &self.0 {
            Root::Empty => Candidate {
                len: 0,
                xor: 0,
                sum: 0,
            },
            Root::Singleton(id) => Candidate {
                len: 1,
                xor: atom(*id, 0x243f_6a88_85a3_08d3),
                sum: atom(*id, 0x1319_8a2e_0370_7344),
            },
            Root::Tree(node) => Candidate {
                len: node.len,
                xor: node.xor,
                sum: node.sum,
            },
        }
    }

    pub(super) fn union(&self, other: &Self) -> Self {
        self.union_inner(other, &mut Counter::new())
    }

    #[cfg(test)]
    pub(super) fn union_counted(&self, other: &Self) -> (Self, Work) {
        let mut counter = Counter::new();
        let result = self.union_inner(other, &mut counter);
        (result, counter.finish())
    }

    fn union_inner(&self, other: &Self, counter: &mut Counter) -> Self {
        if matches!(self.0, Root::Empty) {
            return other.clone();
        }
        if matches!(other.0, Root::Empty) || self.ptr_eq(other) {
            return self.clone();
        }
        if self.candidate() == other.candidate() && self.same_set_inner(other, counter) {
            return self.clone();
        }
        let (mut result, additions) = if self.cardinality() >= other.cardinality() {
            (self.clone(), other)
        } else {
            (other.clone(), self)
        };
        for id in additions.iter() {
            counter.union_item();
            result = result.insert(id, counter);
        }
        result
    }

    pub(super) fn same_set(&self, other: &Self) -> bool {
        self.same_set_inner(other, &mut Counter::new())
    }

    #[cfg(test)]
    pub(super) fn same_set_counted(&self, other: &Self) -> (bool, Work) {
        let mut counter = Counter::new();
        let result = self.same_set_inner(other, &mut counter);
        (result, counter.finish())
    }

    fn same_set_inner(&self, other: &Self, counter: &mut Counter) -> bool {
        if self.ptr_eq(other) {
            return true;
        }
        if self.candidate() != other.candidate() {
            return false;
        }
        match (&self.0, &other.0) {
            (Root::Tree(left), Root::Tree(right)) => {
                compare::same(Some(left), Some(right), counter)
            }
            _ => false,
        }
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Root::Empty, Root::Empty) => true,
            (Root::Singleton(left), Root::Singleton(right)) => left == right,
            (Root::Tree(left), Root::Tree(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    fn insert(&self, id: usize, counter: &mut Counter) -> Self {
        let root = match &self.0 {
            Root::Empty => return Self(Root::Singleton(id)),
            Root::Singleton(existing) => {
                if *existing == id {
                    return self.clone();
                }
                Node::leaf(*existing, counter)
            }
            Root::Tree(root) => Arc::clone(root),
        };
        let routed = route(id);
        let existing = find_leaf(&root, routed);
        if existing == id {
            return self.clone();
        }
        let differing = highest_bit(routed ^ route(existing));
        let leaf = Node::leaf(id, counter);
        Self(Root::Tree(insert_at(
            root, leaf, routed, differing, counter,
        )))
    }

    fn iter(&self) -> Iter<'_> {
        Iter::new(&self.0)
    }
}

impl Node {
    fn leaf(id: usize, counter: &mut Counter) -> Arc<Self> {
        counter.allocated_node();
        Arc::new(Self {
            kind: NodeKind::Leaf(id),
            sample: route(id),
            len: 1,
            xor: atom(id, 0x243f_6a88_85a3_08d3),
            sum: atom(id, 0x1319_8a2e_0370_7344),
        })
    }

    fn branch(bit: u32, left: Arc<Self>, right: Arc<Self>, counter: &mut Counter) -> Arc<Self> {
        assert!(!bit_set(left.sample, bit));
        assert!(bit_set(right.sample, bit));
        counter.allocated_node();
        Arc::new(Self {
            sample: left.sample,
            len: left.len + right.len,
            xor: left.xor ^ right.xor,
            sum: left.sum.wrapping_add(right.sum),
            kind: NodeKind::Branch { bit, left, right },
        })
    }
}

fn find_leaf(root: &Arc<Node>, route: usize) -> usize {
    let mut node = root.as_ref();
    loop {
        match &node.kind {
            NodeKind::Leaf(id) => return *id,
            NodeKind::Branch { bit, left, right } => {
                node = if bit_set(route, *bit) {
                    right.as_ref()
                } else {
                    left.as_ref()
                };
            }
        }
    }
}

fn insert_at(
    root: Arc<Node>,
    leaf: Arc<Node>,
    route: usize,
    bit: u32,
    counter: &mut Counter,
) -> Arc<Node> {
    if let NodeKind::Branch {
        bit: current,
        left,
        right,
    } = &root.kind
        && *current > bit
    {
        return if bit_set(route, *current) {
            Node::branch(
                *current,
                Arc::clone(left),
                insert_at(Arc::clone(right), leaf, route, bit, counter),
                counter,
            )
        } else {
            Node::branch(
                *current,
                insert_at(Arc::clone(left), leaf, route, bit, counter),
                Arc::clone(right),
                counter,
            )
        };
    }
    if bit_set(route, bit) {
        Node::branch(bit, root, leaf, counter)
    } else {
        Node::branch(bit, leaf, root, counter)
    }
}

fn highest_bit(value: usize) -> u32 {
    assert_ne!(value, 0);
    usize::BITS - 1 - value.leading_zeros()
}

fn bit_set(value: usize, bit: u32) -> bool {
    value & (1usize << bit) != 0
}

fn route(id: usize) -> usize {
    id.reverse_bits()
}

fn atom(id: usize, salt: u64) -> u64 {
    let mut value = (id as u64) ^ salt;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
#[path = "cancellation_signature_test.rs"]
mod cancellation_signature_test;
