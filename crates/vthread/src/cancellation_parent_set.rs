//! Compact cancellation ancestry with boxed storage only for multiple parents.

use super::id_set::{IntoIter, Iter};
use crate::id_map::IdHashSet;

#[derive(Clone, Default)]
pub(super) enum ParentSet {
    #[default]
    Empty,
    One(usize),
    Many(Box<IdHashSet<usize>>),
}

impl ParentSet {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Many(ids) => ids.len(),
        }
    }

    pub(super) fn iter(&self) -> Iter<'_> {
        self.into_iter()
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
                *self = Self::Many(Box::new(ids));
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
                    let remaining = *ids
                        .iter()
                        .next()
                        .expect("one remaining cancellation parent");
                    *self = Self::One(remaining);
                }
                true
            }
        }
    }
}

impl FromIterator<usize> for ParentSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut ids = Self::default();
        for id in iter {
            ids.insert(id);
        }
        ids
    }
}

impl<'a> IntoIterator for &'a ParentSet {
    type Item = &'a usize;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ParentSet::Empty => Iter::Empty,
            ParentSet::One(id) => Iter::One(Some(id)),
            ParentSet::Many(ids) => Iter::Many(ids.iter()),
        }
    }
}

impl IntoIterator for ParentSet {
    type Item = usize;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Empty => IntoIter::Empty,
            Self::One(id) => IntoIter::One(Some(id)),
            Self::Many(ids) => IntoIter::Many((*ids).into_iter()),
        }
    }
}

#[cfg(test)]
#[path = "cancellation_parent_set_test.rs"]
mod cancellation_parent_set_test;
