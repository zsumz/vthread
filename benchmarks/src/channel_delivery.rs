//! Exact per-round delivery checking, outside the elapsed benchmark interval.

use crate::config::{Config, Scenario};

pub(crate) struct Delivery {
    received: Vec<Vec<usize>>,
}

impl Delivery {
    pub(crate) fn new(received: Vec<Vec<usize>>) -> Self {
        Self { received }
    }

    fn validate(&self, tasks: usize, per_task: usize) -> Result<(), String> {
        let consumers = tasks / 2;
        let expected = consumers
            .checked_mul(per_task)
            .ok_or("delivery count overflow")?;
        if self.received.len() != consumers {
            return Err("shared channel returned the wrong consumer count".into());
        }
        let mut seen = vec![0_u64; expected.div_ceil(64)];
        for messages in &self.received {
            if messages.len() != per_task {
                return Err("shared channel returned the wrong per-consumer count".into());
            }
            for &value in messages {
                if value >= expected {
                    return Err(format!(
                        "shared channel delivered out-of-range value {value}"
                    ));
                }
                let bit = 1_u64 << (value % 64);
                let word = &mut seen[value / 64];
                if *word & bit != 0 {
                    return Err(format!("shared channel delivered duplicate value {value}"));
                }
                *word |= bit;
            }
        }
        // Exact count, range and uniqueness jointly prove that no value is missing.
        Ok(())
    }
}

pub(crate) fn validate(config: &Config, delivery: Option<&Delivery>) -> Result<(), String> {
    match (config.scenario, delivery) {
        (Scenario::ChannelMpmc { per_task, .. }, Some(delivery)) => {
            delivery.validate(config.tasks, per_task)
        }
        (Scenario::ChannelMpmc { .. }, None) => {
            Err("shared channel delivery evidence missing".into())
        }
        (_, None) => Ok(()),
        (_, Some(_)) => Err("unexpected shared channel delivery evidence".into()),
    }
}

#[cfg(test)]
#[path = "channel_delivery_test.rs"]
mod channel_delivery_test;
