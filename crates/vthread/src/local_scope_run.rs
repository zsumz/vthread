//! Borrowed scope runners share one cancellation and reclamation boundary.

use super::LocalScope;
use crate::{Error, Result, context, control::Shared, error::ScopeRunError};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

/// Runs borrowed work on the current carrier, reclaiming all children before returning.
/// ScopeFailure preserves the body, inherited policy, cleanup and representative child
/// failure with additional counts. Representative order matches run_scope: body, policy,
/// cleanup, then child. A body panic is rethrown unchanged after retaining scope diagnostics.
///
/// ```compile_fail
/// vthread::local_scope(|scope| Ok(scope.spawn("escape", || 1)?));
/// ```
///
/// ```compile_fail
/// vthread::local_scope(|scope| {
///     let too_short = String::from("scope body local");
///     scope.spawn("invalid borrow", || too_short.len())?;
///     Ok(())
/// });
/// ```
///
/// ```compile_fail
/// vthread::local_scope(|scope| {
///     scope.spawn("cannot retain the facade", || scope.cancel())?;
///     Ok(())
/// });
/// ```
pub fn local_scope<'env, R>(
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> Result<R>,
) -> Result<R> {
    run_local(None, body, crate::ScopeFailure::finish)?
}

/// Adds a local deadline; it cannot extend an inherited parent deadline.
pub fn local_scope_with_deadline<'env, R>(
    deadline: Instant,
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> Result<R>,
) -> Result<R> {
    run_local(Some(deadline), body, crate::ScopeFailure::finish)?
}

/// Runs borrowed children with an application-specific, caller-owned body error.
/// Body failure cancels and reclaims children before returning; diagnostics never
/// retain or format the application error.
pub fn try_local_scope<'env, R, E>(
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> std::result::Result<R, E>,
) -> std::result::Result<R, ScopeRunError<E>> {
    run_local(None, body, crate::ScopeFailure::finish_generic).map_err(ScopeRunError::Runtime)?
}

/// Adds a local deadline to a generic-error scope without extending its parent deadline.
pub fn try_local_scope_with_deadline<'env, R, E>(
    deadline: Instant,
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> std::result::Result<R, E>,
) -> std::result::Result<R, ScopeRunError<E>> {
    run_local(Some(deadline), body, crate::ScopeFailure::finish_generic)
        .map_err(ScopeRunError::Runtime)?
}

fn run_local<'env, R, E, O>(
    deadline: Option<Instant>,
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> std::result::Result<R, E>,
    finish: impl FnOnce(crate::ScopeFailure, std::result::Result<R, E>, Result<()>, &Shared) -> O,
) -> Result<O> {
    let mounted = context::current().ok_or(Error::OutsideVThread)?;
    let execution = std::rc::Rc::clone(mounted.execution()?);
    execution.data.check()?;
    let options = execution.data.options().child(deadline);
    Ok(vthread_stack::fiber_scope(
        execution.shared().config.max_vthreads(),
        |fibers| {
            let scope = LocalScope {
                fibers,
                execution,
                options,
                records: RefCell::new(Vec::new()),
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| body(&scope)));
            let policy = scope.options.check();
            if !matches!(&outcome, Ok(Ok(_))) || policy.is_err() {
                scope.cancel();
            }
            let failures = scope.drain();
            let policy = if matches!(&outcome, Ok(Ok(_))) {
                policy.and_then(|()| scope.options.check())
            } else {
                policy
            };
            match outcome {
                Err(payload) => failures.unwind(payload, policy, scope.execution.shared()),
                Ok(result) => finish(failures, result, policy, scope.execution.shared()),
            }
        },
    ))
}

#[cfg(test)]
#[path = "local_scope_run_test.rs"]
mod local_scope_run_test;
