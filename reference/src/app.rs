//! Application-owned error handling preserves independent work and shutdown failures.

use vthread::{Error, Runtime, RuntimeBuilder};

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) body: Option<Box<Error>>,
    pub(crate) shutdown: Option<Box<Error>>,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "application: {:?}; shutdown: {:?}",
            self.body, self.shutdown
        )
    }
}
impl std::error::Error for Failure {}

pub(crate) fn run<T>(
    builder: RuntimeBuilder,
    body: impl FnOnce(&Runtime) -> vthread::Result<T>,
) -> Result<T, Failure> {
    let runtime = builder.build().map_err(|error| Failure {
        body: Some(Box::new(error)),
        shutdown: None,
    })?;
    let result = body(&runtime);
    finish(result, runtime.shutdown().map(|_| ()))
}

fn finish<T>(body: vthread::Result<T>, shutdown: vthread::Result<()>) -> Result<T, Failure> {
    match (body, shutdown) {
        (Ok(value), Ok(())) => Ok(value),
        (body, shutdown) => Err(Failure {
            body: body.err().map(Box::new),
            shutdown: shutdown.err().map(Box::new),
        }),
    }
}

#[cfg(test)]
#[path = "app_test.rs"]
mod app_test;
