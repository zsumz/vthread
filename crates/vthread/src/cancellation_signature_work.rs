//! Test-only accounting for exact signature operations.

#[cfg(test)]
use super::Work;

pub(super) struct Counter {
    #[cfg(test)]
    work: Work,
}

impl Default for Counter {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Counter {
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            #[cfg(test)]
            work: Work::default(),
        }
    }

    #[inline]
    pub(super) fn union_item(&mut self) {
        #[cfg(test)]
        {
            self.work.union_items += 1;
        }
    }

    #[inline]
    pub(super) fn equality_node(&mut self) {
        #[cfg(test)]
        {
            self.work.equality_nodes += 1;
        }
    }

    #[inline]
    pub(super) fn allocated_node(&mut self) {
        #[cfg(test)]
        {
            self.work.allocated_nodes += 1;
        }
    }

    #[cfg(test)]
    pub(super) fn finish(self) -> Work {
        self.work
    }
}

#[cfg(test)]
#[path = "cancellation_signature_work_test.rs"]
mod cancellation_signature_work_test;
