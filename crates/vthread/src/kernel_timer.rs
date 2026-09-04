//! Direct, generation-checked expiry of carrier-owned timers.

use std::time::Instant;

use super::Kernel;
use crate::Result;

impl Kernel {
    // Keep the timer-tree walk out of the always-hot dispatch body.
    #[inline(never)]
    pub(super) fn expire_timers(&mut self) -> Result<()> {
        for expired in self.timers.pop_expired(Instant::now()) {
            let token = expired.token;
            #[cfg(feature = "runtime-evidence")]
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::TimerRetired {
                    wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                    carrier: self.id,
                    reason: crate::diagnostics::evidence::TimerRetirement::Expired,
                },
            );
            let Some(parked) = self
                .parked
                .get(expired.task)
                .filter(|parked| parked.token == token)
            else {
                continue;
            };
            if let Some(registration) = &parked.registration {
                registration.select_timeout(token)?;
            } else {
                self.task(parked.task)
                    .execution()
                    .select_synchronization_timeout(token)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "kernel_timer_test.rs"]
mod kernel_timer_test;
