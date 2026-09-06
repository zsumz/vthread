//! Retain registration guards and failures until the exact publication can retire.

use crate::{
    Error, Result,
    context::Execution,
    wait::{WaitCell, WaitRegistration, WakeCause},
};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use vthread_stack::{ParkRequest, ParkToken, Suspension};

pub(super) fn park<const PLAIN_READY: bool, const PERMIT_READY: bool, G>(
    execution: &Execution,
    wait: &WaitCell,
    request: ParkRequest,
    mut registration: Option<WaitRegistration>,
    register: impl FnOnce(ParkToken, Option<&WaitRegistration>) -> Result<G>,
) -> Result<WakeCause> {
    let token = request.token();
    let mut generation = wait.guard(token);
    // These guards must outlive the caught failure, not unwind before the owner
    // can defer its generation. The normal success path still attaches once.
    let mut subscription = None;
    let mut external = None;
    let mut publication = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let policy = &execution.data;
        if policy.masked() == 0 {
            subscription = Some(match registration.as_ref() {
                Some(registration) => policy.cancellation().register(token, registration)?,
                None => policy.cancellation().register_resident(token, wait)?,
            });
        }
        external = Some(register(token, registration.as_ref())?);
        if let Some(registration) = registration.take() {
            publication = Some(execution.publish_wait(token, registration)?);
        }
        vthread_stack::suspend(Suspension::Park(request)).map_err(Error::from)?;
        if PLAIN_READY {
            wait.finish_plain_ready(token)
        } else if PERMIT_READY {
            wait.finish_permit_ready(token)
        } else {
            wait.finish(token)
        }
    }));
    if !matches!(outcome, Ok(Ok(_))) {
        drop(publication.take());
        retire_after_failure(execution, wait, token);
    }
    generation.disarm();
    match outcome {
        Ok(result) => result,
        // In particular, never wrap or replace the engine's forced-unwind token.
        Err(payload) => resume_unwind(payload),
    }
}

#[cold]
fn retire_after_failure(execution: &Execution, wait: &WaitCell, token: ParkToken) {
    while !wait.try_rollback(token) {
        // This is not a public yield/checkpoint and never suspends an active
        // unwind. The original error/payload and all registration guards remain
        // stack-owned. Forced ancestor cleanup must preflight publication first.
        assert!(
            !execution.data.closing(),
            "forced cleanup retained an incomplete publication"
        );
        assert!(
            !std::thread::panicking(),
            "wait retirement attempted to suspend an unwind"
        );
        let _publication = execution
            .publish_wait(token, wait.registration())
            .expect("failed park owns its sole pending generation");
        vthread_stack::suspend(Suspension::Park(ParkRequest::new(token, None)))
            .expect("mounted owner can defer failed registration cleanup");
    }
}

#[cfg(test)]
#[path = "parking_wait_test.rs"]
mod parking_wait_test;
