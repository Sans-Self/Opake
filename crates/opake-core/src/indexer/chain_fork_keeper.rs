//! Transient pub-sub for `chain:forked` SSE events.
//!
//! Unlike [`TreeKeeper`], [`WorkspaceKeeper`], and [`InboxKeeper`], this
//! one holds *no state*. Chain-fork events are signals — there is nothing
//! to remember between firings. The keeper exists purely to fan out
//! incoming events to whatever watchers are registered.
//!
//! Wiring: the SSE consumer dispatches `SseEvent::ChainForked` payloads
//! to [`ChainForkKeeper::dispatch`]; subscribers (typically the React
//! layer's retry-on-fork hook) register via
//! [`ChainForkKeeper::install_watcher`].
//!
//! [`TreeKeeper`]: crate::indexer::tree_keeper::TreeKeeper
//! [`WorkspaceKeeper`]: crate::indexer::workspace_keeper::WorkspaceKeeper
//! [`InboxKeeper`]: crate::indexer::inbox_keeper::InboxKeeper

use std::collections::HashMap;

use crate::indexer::sse::events::SseChainForked;

/// Opaque handle returned by [`ChainForkKeeper::install_watcher`]. Pass to
/// [`ChainForkKeeper::unwatch`] to stop receiving notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChainForkWatcherHandle(u64);

/// Callback fired for each chain-fork event. Must not re-enter the keeper
/// (calling `install_watcher` or `unwatch` from inside the callback would
/// invalidate the iterator).
pub type ChainForkWatcherCallback = Box<dyn FnMut(&SseChainForked)>;

/// Routes `chain:forked` events to registered subscribers.
pub struct ChainForkKeeper {
    watchers: HashMap<ChainForkWatcherHandle, ChainForkWatcherCallback>,
    next_watcher_id: u64,
}

impl ChainForkKeeper {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
            next_watcher_id: 0,
        }
    }

    /// Register a callback. Returns a handle for [`Self::unwatch`].
    pub fn install_watcher(&mut self, cb: ChainForkWatcherCallback) -> ChainForkWatcherHandle {
        let id = self.next_watcher_id;
        self.next_watcher_id += 1;
        let handle = ChainForkWatcherHandle(id);
        self.watchers.insert(handle, cb);
        handle
    }

    /// Remove a watcher. Idempotent — unwatching an unknown handle is a no-op.
    pub fn unwatch(&mut self, handle: ChainForkWatcherHandle) {
        self.watchers.remove(&handle);
    }

    /// Fire every registered callback with the event. Order is unspecified —
    /// the HashMap iteration order is undefined and the keeper doesn't promise
    /// FIFO delivery.
    pub fn dispatch(&mut self, event: &SseChainForked) {
        for cb in self.watchers.values_mut() {
            cb(event);
        }
    }

    /// Drop every registered watcher. Used on logout / account switch via
    /// `wipeState()` so callbacks holding closures over the prior account's
    /// state don't fire against fresh sessions.
    pub fn uninstall_all(&mut self) {
        self.watchers.clear();
    }

    /// Number of active subscribers. Test helper.
    #[cfg(test)]
    pub fn watcher_count(&self) -> usize {
        self.watchers.len()
    }
}

impl Default for ChainForkKeeper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn sample_event() -> SseChainForked {
        SseChainForked {
            workspace_id: "at://did:plc:alice/at.opake.keyring/kr1".into(),
            scope: "directory".into(),
            path: Some("/q1/".into()),
            your_uri: "at://did:plc:bob/at.opake.directory/loser".into(),
            fork_point_uri: "at://did:plc:alice/at.opake.directory/head".into(),
            winner_uri: "at://did:plc:carol/at.opake.directory/winner".into(),
            winner_cid: "bafywinner".into(),
        }
    }

    #[test]
    fn dispatches_to_all_watchers() {
        let mut keeper = ChainForkKeeper::new();
        let fires = Rc::new(RefCell::new(0usize));

        for _ in 0..3 {
            let counter = Rc::clone(&fires);
            keeper.install_watcher(Box::new(move |_| {
                *counter.borrow_mut() += 1;
            }));
        }

        keeper.dispatch(&sample_event());
        assert_eq!(*fires.borrow(), 3);
    }

    #[test]
    fn unwatch_stops_callbacks() {
        let mut keeper = ChainForkKeeper::new();
        let fired = Rc::new(RefCell::new(false));

        let flag = Rc::clone(&fired);
        let handle = keeper.install_watcher(Box::new(move |_| {
            *flag.borrow_mut() = true;
        }));

        keeper.unwatch(handle);
        keeper.dispatch(&sample_event());
        assert!(!*fired.borrow());
    }

    #[test]
    fn uninstall_all_clears_watchers() {
        let mut keeper = ChainForkKeeper::new();
        keeper.install_watcher(Box::new(|_| {}));
        keeper.install_watcher(Box::new(|_| {}));
        assert_eq!(keeper.watcher_count(), 2);

        keeper.uninstall_all();
        assert_eq!(keeper.watcher_count(), 0);
    }
}
