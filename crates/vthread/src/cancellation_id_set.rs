//! Deterministic hashed cancellation adjacency with zero allocation for one edge.

use crate::id_map::IdHashSet;
use std::collections::hash_set;

#[derive(Clone, Default)]
pub(super) enum IdSet {
    #[default]
    Empty,
    One(usize),
    Many(IdHashSet<usize>),
}

impl IdSet {
    pub(super) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Many(ids) => ids.len(),
        }
    }

    #[cfg(test)]
    pub(super) fn contains(&self, id: &usize) -> bool {
        match self {
            Self::Empty => false,
            Self::One(existing) => existing == id,
            Self::Many(ids) => ids.contains(id),
        }
    }

    pub(super) fn insert(&mut self, id: usize) -> bool {
        match self {
            Self::Empty => {
                *self = Self::One(id);
                true
            }
            Self::One(existing) if *existing == id => false,
            Self::One(existing) => {
                let mut ids = IdHashSet::default();
                ids.insert(*existing);
                ids.insert(id);
                *self = Self::Many(ids);
                true
            }
            Self::Many(ids) => ids.insert(id),
        }
    }

    pub(super) fn remove(&mut self, id: &usize) -> bool {
        match self {
            Self::Empty => false,
            Self::One(existing) if existing == id => {
                *self = Self::Empty;
                true
            }
            Self::One(_) => false,
            Self::Many(ids) => {
                if !ids.remove(id) {
                    return false;
                }
                if ids.len() == 1 {
                    let remaining = *ids.iter().next().expect("one remaining cancellation edge");
                    *self = Self::One(remaining);
                }
                true
            }
        }
    }

    pub(super) fn iter(&self) -> Iter<'_> {
        self.into_iter()
    }

    pub(super) fn extend_set(&mut self, other: &Self) {
        for id in other {
            self.insert(*id);
        }
    }
}

impl FromIterator<usize> for IdSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut ids = Self::default();
        for id in iter {
            ids.insert(id);
        }
        ids
    }
}

pub(super) enum Iter<'a> {
    Empty,
    One(Option<&'a usize>),
    Many(hash_set::Iter<'a, usize>),
}

impl<'a> IntoIterator for &'a IdSet {
    type Item = &'a usize;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            IdSet::Empty => Iter::Empty,
            IdSet::One(id) => Iter::One(Some(id)),
            IdSet::Many(ids) => Iter::Many(ids.iter()),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::One(id) => id.take(),
            Self::Many(ids) => ids.next(),
        }
    }
}

pub(super) enum IntoIter {
    Empty,
    One(Option<usize>),
    Many(hash_set::IntoIter<usize>),
}

impl IntoIterator for IdSet {
    type Item = usize;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Empty => IntoIter::Empty,
            Self::One(id) => IntoIter::One(Some(id)),
            Self::Many(ids) => IntoIter::Many(ids.into_iter()),
        }
    }
}

impl Iterator for IntoIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::One(id) => id.take(),
            Self::Many(ids) => ids.next(),
        }
    }
}

#[cfg(test)]
#[path = "cancellation_id_set_test.rs"]
mod cancellation_id_set_test;
