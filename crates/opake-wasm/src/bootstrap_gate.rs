//! Snapshot/stream sequencing for the SSE consumer.
//!
//! `listWorkspaces` / `listInbox` fetch a point-in-time snapshot from the
//! indexer and then call the keeper's wholesale `bootstrap()`. Meanwhile
//! the SSE consumer applies live deltas to the same keeper. If a delta
//! lands during the fetch, the stale snapshot clobbers it — a delete
//! arriving mid-fetch gets silently restored.
//!
//! `BootstrapGate` closes that window: open it before the fetch, and SSE
//! events that arrive are buffered instead of applied; after the snapshot
//! installs, the buffered events replay on top. Buffered events are
//! idempotent against the snapshot, so any the snapshot already reflects
//! no-op on replay.
//!
//! This is *client-sync policy*, deliberately kept out of opake-core: the
//! race is a property of this client consuming a snapshot endpoint and a
//! delta stream at once, not of the Opake protocol — the CLI syncs
//! without ever hitting it. The keeper stays a dumb projection; sequencing
//! is the consumer's job.

use opake_core::indexer::sse::events::SseEvent;

/// Buffers SSE events that arrive while a snapshot fetch is in flight.
pub(crate) struct BootstrapGate {
    active: bool,
    buffered: Vec<SseEvent>,
}

impl BootstrapGate {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            buffered: Vec::new(),
        }
    }

    /// Open the gate before awaiting the snapshot fetch. Events that
    /// arrive while open are buffered instead of applied live. A fresh
    /// open drops any buffer left over from an abandoned cycle.
    pub(crate) fn begin(&mut self) {
        self.active = true;
        self.buffered.clear();
    }

    /// If the gate is open, clone the event into the buffer and report
    /// `true` (the caller must not apply it live — the snapshot would
    /// clobber it). When closed, report `false` (apply normally).
    pub(crate) fn capture_if_active(&mut self, event: &SseEvent) -> bool {
        if self.active {
            self.buffered.push(event.clone());
            true
        } else {
            false
        }
    }

    /// Close the gate and take the buffered events for replay.
    pub(crate) fn finish(&mut self) -> Vec<SseEvent> {
        self.active = false;
        std::mem::take(&mut self.buffered)
    }
}

#[cfg(test)]
mod tests {
    use super::BootstrapGate;
    use opake_core::indexer::sse::events::SseEvent;

    #[test]
    fn passes_events_through_when_inactive() {
        let mut gate = BootstrapGate::new();
        // No bootstrap in flight: events apply live, nothing buffered.
        assert!(!gate.capture_if_active(&SseEvent::Reconnect));
        assert!(gate.finish().is_empty());
    }

    #[test]
    fn buffers_while_active_and_drains_on_finish() {
        let mut gate = BootstrapGate::new();
        gate.begin();
        assert!(gate.capture_if_active(&SseEvent::Reconnect));
        assert!(gate.capture_if_active(&SseEvent::Reconnect));

        let drained = gate.finish();
        assert_eq!(drained.len(), 2, "both buffered events must replay");

        // Gate is closed again: subsequent events pass through live.
        assert!(!gate.capture_if_active(&SseEvent::Reconnect));
    }

    #[test]
    fn begin_drops_a_stale_buffer() {
        let mut gate = BootstrapGate::new();
        gate.begin();
        gate.capture_if_active(&SseEvent::Reconnect);
        // A second bootstrap (e.g. a reconnect re-sync) starts clean — a
        // buffer left over from an abandoned cycle must not leak into it.
        gate.begin();
        assert!(gate.finish().is_empty());
    }
}
