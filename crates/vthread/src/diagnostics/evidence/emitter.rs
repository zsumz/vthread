//! Runtime and carrier context shared by evidence-producing scheduler components.

#[derive(::core::clone::Clone)]
pub(crate) struct Emitter {
    runtime: crate::diagnostics::RuntimeId,
    carrier: crate::CarrierId,
    recorder: super::Recorder,
}

impl Emitter {
    pub(crate) fn new(
        runtime: crate::diagnostics::RuntimeId,
        carrier: crate::CarrierId,
        recorder: super::Recorder,
    ) -> Self {
        Self {
            runtime,
            carrier,
            recorder,
        }
    }

    pub(crate) fn record(&self, kind: super::RuntimeEventKind) {
        self.recorder.record(self.runtime, kind);
    }

    pub(crate) fn carrier(&self) -> crate::CarrierId {
        self.carrier
    }
}

#[cfg(test)]
#[path = "emitter_test.rs"]
mod emitter_test;
