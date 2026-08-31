//! Explicit shutdown and lossless result merging for the application runner.

use crate::{Error, Result, Runtime, Scope, error::RunFailure};

pub(crate) fn run<R>(runtime: Runtime, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
    let scope = runtime.run_scope(body);
    let shutdown = runtime.shutdown();
    match (scope, shutdown) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(scope), Ok(_)) => Err(scope),
        (Ok(_), Err(shutdown)) => Err(shutdown),
        (Err(scope), Err(shutdown)) => {
            Err(Error::RunFailed(Box::new(RunFailure::new(scope, shutdown))))
        }
    }
}

#[cfg(test)]
#[path = "runner_test.rs"]
mod runner_test;
