//! Native results are transferred without running destructors under a metadata lock.

use crate::{Error, Result, signal::lock};
use std::sync::Mutex;

pub(super) struct Output<T> {
    value: Mutex<Option<Result<T>>>,
}
impl<T> Output<T> {
    pub(super) fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }
    pub(super) fn store(&self, result: Result<T>) {
        *lock(&self.value) = Some(result);
    }
    pub(super) fn take(&self) -> Result<T> {
        lock(&self.value).take().ok_or(Error::fault(
            crate::error::FaultComponent::Native,
            "native work woke without a result",
        ))?
    }
}

#[cfg(test)]
#[path = "result_test.rs"]
mod result_test;
