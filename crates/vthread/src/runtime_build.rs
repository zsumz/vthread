//! Runtime construction and explicit component initialization.

use super::{Runtime, runtime_lifecycle};
use crate::{CarrierId, Error, Result, RuntimeConfig, carrier, context, control::Shared};
use std::{sync::Arc, thread};

impl Runtime {
    pub(crate) fn from_config(config: RuntimeConfig) -> Result<Self> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideManagedWorker);
        }
        let shared = Arc::new(Shared::new(config));
        let shutdown_driver = runtime_lifecycle::ShutdownDriver::new(&shared)?;
        let runtime = Self {
            config,
            shared,
            shutdown_driver,
        };
        let initialized = runtime
            .initialize()
            .and_then(|()| crate::lifecycle_owner::check_health());
        runtime.shutdown_driver.ready(&runtime.shared);
        match initialized {
            Ok(()) => Ok(runtime),
            Err(construction) => match runtime.shutdown() {
                Ok(_) => Err(construction),
                Err(shutdown) => Err(Error::ConstructionFailed(Box::new(
                    crate::error::RuntimeBuildFailure::new(construction, shutdown),
                ))),
            },
        }
    }

    fn initialize(&self) -> Result<()> {
        #[cfg(test)]
        runtime_build_test::inject(self, 0)?;
        self.shared
            .services
            .set(crate::services::Services::new(
                self.config,
                Arc::downgrade(&self.shared),
            )?)
            .map_err(|_| {
                Error::fault(
                    crate::error::FaultComponent::Lifecycle,
                    "runtime services initialized twice",
                )
            })?;
        for index in 0..self.config.carriers() {
            #[cfg(test)]
            runtime_build_test::inject(self, index + 1)?;
            let shared = Arc::clone(&self.shared);
            let name = format!("vthread-carrier-{index}");
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || {
                    crate::worker_context::attach(
                        Arc::downgrade(&shared),
                        crate::ThreadComponent::Carrier,
                    );
                    carrier::run(Arc::clone(&shared), CarrierId(index));
                    #[cfg(test)]
                    if let Some(hook) = crate::signal::lock(&shared.carrier_exit_hook).take() {
                        hook();
                    }
                })
                .map_err(|error| Error::thread_start(crate::ThreadComponent::Carrier, error))?;
            self.shared.inboxes[index]
                .started
                .store(true, std::sync::atomic::Ordering::Release);
            self.shared.resources.workers.push(worker);
        }
        #[cfg(test)]
        runtime_build_test::inject(self, self.config.carriers() + 1)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_build_test.rs"]
mod runtime_build_test;
