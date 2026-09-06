//! Type-only identities; mutable synchronization comes from production source.

use super::WakeCause;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParkToken {
    wait: u64,
    generation: u64,
}
impl ParkToken {
    pub(crate) fn new(wait: u64, generation: u64) -> Self {
        Self { wait, generation }
    }
    pub(crate) fn wait(self) -> u64 {
        self.wait
    }
    pub(crate) fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskId(u64);
impl TaskId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskKey(usize);
impl TaskKey {
    pub(crate) fn owned(index: usize) -> Self {
        Self((index + 1) * 2)
    }
    pub(crate) fn encoded(self) -> usize {
        self.0
    }
    pub(crate) fn from_encoded(encoded: usize) -> Self {
        assert_ne!(encoded, 0);
        Self(encoded)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WakeNotice {
    pub(crate) token: ParkToken,
    pub(crate) task: TaskId,
    pub(crate) route: TaskKey,
    pub(crate) cause: WakeCause,
}
