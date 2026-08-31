//! Streaming operator diagnostics; no native stack inspection or user callbacks under locks.

use crate::{RuntimeSnapshot, TaskSnapshot};
use std::fmt::{self, Write};

/// Accounting for one bounded dump write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DumpReport {
    bytes: usize,
    truncated: bool,
}
impl DumpReport {
    /// Bytes successfully written, never exceeding 64 KiB.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Whether rows or text were omitted at the output budget.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}
impl RuntimeSnapshot {
    /// Writes a human-readable task dump capped at 64 KiB, including a truncation marker.
    ///
    /// Names and error text are escaped. Carrier counters are independently published,
    /// so this is an observation, not a globally atomic scheduler trace. No stack frames
    /// are unwound. The returned report explicitly identifies omitted output.
    /// The destination can reject writes with fmt::Error; no success report is returned then.
    pub fn write_dump(&self, output: &mut impl Write) -> Result<DumpReport, fmt::Error> {
        const MARKER: &str = "\n[dump truncated]\n";
        let mut buffer = crate::diagnostic_text::BoundedText::new(64 * 1024 - MARKER.len());
        let _ = self.render_dump(&mut buffer);
        if buffer.truncated {
            buffer.text.push_str(MARKER);
        }
        output.write_str(&buffer.text)?;
        Ok(DumpReport {
            bytes: buffer.text.len(),
            truncated: buffer.truncated,
        })
    }
    fn render_dump(&self, output: &mut impl Write) -> fmt::Result {
        writeln!(
            output,
            "vthread dump v2 runtime={} active={} runnable={} parked={} timers={} accepting={} shutdown={:?}",
            self.runtime_id,
            self.active,
            self.runnable,
            self.parked,
            self.timers,
            self.accepting,
            self.shutdown_phase
        )?;
        for carrier in &self.carriers {
            writeln!(
                output,
                "carrier={} status={:?} active={} runnable={} parked={} timers={} starts={} wakes={}",
                carrier.id.index(),
                carrier.status,
                carrier.active,
                carrier.runnable,
                carrier.parked,
                carrier.timers,
                carrier.pending_starts,
                carrier.pending_wakes
            )?;
        }
        let io = &self.services;
        writeln!(
            output,
            "services readiness={}/{} installed={} failed={} error={:?} blocking_queued={} blocking_running={} blocking_discarding={} blocking_capacity={} blocking_panics={}",
            io.readiness_waits,
            io.readiness_capacity,
            io.readiness_registered,
            io.readiness_failed,
            io.readiness_error,
            io.blocking_queued,
            io.blocking_running,
            io.blocking_discarding,
            io.blocking_capacity,
            io.blocking_panics
        )?;
        for task in &self.tasks {
            task_line(output, "task", task)?;
        }
        if let Some(stall) = &self.last_stall {
            writeln!(
                output,
                "last_stall scope={} quiescent_ms={} tasks={}",
                stall.scope,
                stall.quiescent_for.as_millis(),
                stall.tasks.len()
            )?;
            for task in &stall.tasks {
                task_line(output, "stalled_task", task)?;
            }
        }
        Ok(())
    }
}

fn task_line(output: &mut impl Write, kind: &str, task: &TaskSnapshot) -> fmt::Result {
    writeln!(
        output,
        "{kind} id={} name={:?} scope={} parent={:?} carrier={} status={:?} wait={:?} wake={:?} deadline={:?} inherited_deadline={:?} cancelled={} failure={:?} mounts={} yields={} parks={} observed={}",
        task.id,
        task.name,
        task.scope,
        task.parent,
        task.carrier.index(),
        task.status,
        task.last_suspension,
        task.last_wake,
        task.deadline,
        task.inherited_deadline,
        task.cancellation_requested,
        task.failure,
        task.mounts,
        task.yields,
        task.parks,
        task.outcome_observed
    )
}

#[cfg(test)]
#[path = "dump_test.rs"]
mod dump_test;
