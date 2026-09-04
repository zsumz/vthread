//! Inline-first storage for active cancellation generations.

use super::WaitSlot;
use crate::id_map::IdMap;

#[derive(Default)]
pub(super) enum WaitShard {
    #[default]
    Empty,
    One {
        node: usize,
        wait: WaitSlot,
    },
    Many(IdMap<usize, WaitSlot>),
}

impl WaitShard {
    #[inline]
    pub(super) fn get(&self, node: usize) -> Option<&WaitSlot> {
        match self {
            Self::Empty => None,
            Self::One {
                node: occupied,
                wait,
            } => (*occupied == node).then_some(wait),
            Self::Many(waits) => waits.get(&node),
        }
    }

    pub(super) fn try_insert(&mut self, node: usize, wait: WaitSlot) -> bool {
        match self {
            Self::Empty => {
                *self = Self::One { node, wait };
                true
            }
            Self::One { node: occupied, .. } if *occupied == node => {
                drop(wait);
                false
            }
            Self::One { .. } => {
                let Self::One {
                    node: previous_node,
                    wait: previous_wait,
                } = std::mem::replace(self, Self::Empty)
                else {
                    unreachable!()
                };
                let mut waits = IdMap::default();
                waits.reserve(2);
                assert!(waits.insert(previous_node, previous_wait).is_none());
                assert!(waits.insert(node, wait).is_none());
                *self = Self::Many(waits);
                true
            }
            Self::Many(waits) => match waits.entry(node) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(wait);
                    true
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    drop(wait);
                    false
                }
            },
        }
    }

    pub(super) fn remove(&mut self, node: usize, token: vthread_stack::ParkToken) {
        match self {
            Self::Empty => {}
            Self::One {
                node: occupied,
                wait,
            } if *occupied != node || wait.token != token => {}
            Self::One { .. } => {
                let Self::One { wait, .. } = std::mem::replace(self, Self::Empty) else {
                    unreachable!()
                };
                drop(wait);
            }
            // Keep promoted storage allocated for workloads whose live IDs
            // repeatedly collide in this shard.
            Self::Many(waits) => {
                if let std::collections::hash_map::Entry::Occupied(entry) = waits.entry(node)
                    && entry.get().token == token
                {
                    drop(entry.remove());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "cancellation_wait_shard_test.rs"]
mod cancellation_wait_shard_test;
