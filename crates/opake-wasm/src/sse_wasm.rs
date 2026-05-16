// WASM bindings for the SSE consumer, tree watchers, and workspace-list
// watchers.
//
// Exposes:
//   - WasmOpakeHandle::startSseConsumer(indexerUrl)
//   - WasmOpakeHandle::stopSseConsumer()
//   - WasmOpakeHandle::watchWorkspaces(callback)
//   - WasmFileManagerHandle::watchDirectory(uri, callback)
//   - WasmDirectoryWatcher::close()
//   - WasmWorkspaceWatcher::close()
//
// The consumer loop runs via wasm_bindgen_futures::spawn_local and pulls
// events from the browser's EventSource (WasmSseTransport). Record
// events are dispatched to both the TreeKeeper (directory tree state)
// and the WorkspaceKeeper (workspace-list state).

use std::rc::Rc;
use std::time::Duration;

use futures_util::lock::Mutex;
use opake_core::directories::DirectoryTree;
use opake_core::indexer::chain_fork_keeper::{
    ChainForkKeeper, ChainForkWatcherCallback, ChainForkWatcherHandle,
};
use opake_core::indexer::inbox_keeper::{
    self as ik, InboxKeeper, InboxSnapshot, InboxWatcherCallback, InboxWatcherHandle,
};
use opake_core::indexer::request_sse_token;
use opake_core::indexer::sse::consumer::{JitterRng, SleepFn, SseConsumer, TokenFetcher};
use opake_core::indexer::sse::events::SseEvent;
use opake_core::indexer::sse::wasm_connection::WasmSseTransport;
use opake_core::indexer::tree_keeper::{TreeKeeper, WatcherCallback, WatcherHandle};
use opake_core::indexer::workspace_keeper::{
    self as wk, WorkspaceKeeper, WorkspaceSnapshot, WorkspaceWatcherCallback,
    WorkspaceWatcherHandle,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::file_manager_wasm::WasmFileManagerHandle;
use crate::opake_wasm::WasmOpakeHandle;
use crate::wasm_util::{build_snapshot, wasm_err, WasmOpake};

// ---------------------------------------------------------------------------
// WasmDirectoryWatcher — returned by watchDirectory, exposes close()
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = DirectoryWatcher)]
pub struct WasmDirectoryWatcher {
    tree_keeper: Rc<Mutex<TreeKeeper>>,
    handle: WatcherHandle,
    closed: Rc<std::cell::Cell<bool>>,
}

#[wasm_bindgen(js_class = DirectoryWatcher)]
impl WasmDirectoryWatcher {
    /// Stop receiving notifications. Idempotent.
    pub async fn close(&self) {
        if self.closed.get() {
            return;
        }
        self.closed.set(true);
        let mut keeper = self.tree_keeper.lock().await;
        keeper.unwatch(self.handle);
    }
}

// ---------------------------------------------------------------------------
// WasmWorkspaceWatcher — returned by watchWorkspaces, exposes close()
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = WorkspaceWatcher)]
pub struct WasmWorkspaceWatcher {
    workspace_keeper: Rc<Mutex<WorkspaceKeeper>>,
    handle: WorkspaceWatcherHandle,
    closed: Rc<std::cell::Cell<bool>>,
}

#[wasm_bindgen(js_class = WorkspaceWatcher)]
impl WasmWorkspaceWatcher {
    /// Stop receiving notifications. Idempotent.
    pub async fn close(&self) {
        if self.closed.get() {
            return;
        }
        self.closed.set(true);
        let mut keeper = self.workspace_keeper.lock().await;
        keeper.unwatch(self.handle);
    }
}

// ---------------------------------------------------------------------------
// WasmInboxWatcher — returned by watchInbox, exposes close()
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = InboxWatcher)]
pub struct WasmInboxWatcher {
    inbox_keeper: Rc<Mutex<InboxKeeper>>,
    handle: InboxWatcherHandle,
    closed: Rc<std::cell::Cell<bool>>,
}

#[wasm_bindgen(js_class = InboxWatcher)]
impl WasmInboxWatcher {
    /// Stop receiving notifications. Idempotent.
    pub async fn close(&self) {
        if self.closed.get() {
            return;
        }
        self.closed.set(true);
        let mut keeper = self.inbox_keeper.lock().await;
        keeper.unwatch(self.handle);
    }
}

// ---------------------------------------------------------------------------
// WasmChainForkWatcher — returned by watchChainForks, exposes close()
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = ChainForkWatcher)]
pub struct WasmChainForkWatcher {
    chain_fork_keeper: Rc<Mutex<ChainForkKeeper>>,
    handle: ChainForkWatcherHandle,
    closed: Rc<std::cell::Cell<bool>>,
}

#[wasm_bindgen(js_class = ChainForkWatcher)]
impl WasmChainForkWatcher {
    /// Stop receiving notifications. Idempotent.
    pub async fn close(&self) {
        if self.closed.get() {
            return;
        }
        self.closed.set(true);
        let mut keeper = self.chain_fork_keeper.lock().await;
        keeper.unwatch(self.handle);
    }
}

// ---------------------------------------------------------------------------
// WasmOpakeHandle::watchWorkspaces
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_class = OpakeContext)]
impl WasmOpakeHandle {
    /// Subscribe to live changes in the workspace list.
    ///
    /// Fires the callback once immediately with the current snapshot
    /// (which has `loaded = false` and an empty entry list if the
    /// keeper hasn't been bootstrapped yet), and again on every change.
    ///
    /// The callback receives a `WorkspaceSnapshot` with shape
    /// `{ entries: WorkspaceEntry[], loaded: bool }`.
    ///
    /// Returns a `WorkspaceWatcher` handle. Call `.close()` to
    /// unsubscribe (typically from a React useEffect cleanup).
    ///
    /// The keeper is populated by:
    ///   - `listWorkspaces()` (bootstrap — replaces the entry set)
    ///   - SSE `keyring:upsert` / `keyring:delete` events (incremental)
    ///
    /// If the keeper hasn't been bootstrapped, the initial snapshot has
    /// `loaded == false` — subscribers should treat this as "loading"
    /// state and show a placeholder until the next fire brings `loaded == true`.
    #[wasm_bindgen(js_name = watchWorkspaces)]
    pub async fn watch_workspaces(
        &self,
        callback: js_sys::Function,
    ) -> Result<WasmWorkspaceWatcher, JsError> {
        let cb = js_workspace_watcher_callback(callback);
        let mut keeper = self.workspace_keeper.lock().await;
        let handle = keeper.install_watcher(cb);
        Ok(WasmWorkspaceWatcher {
            workspace_keeper: Rc::clone(&self.workspace_keeper),
            handle,
            closed: Rc::new(std::cell::Cell::new(false)),
        })
    }

    /// Subscribe to live changes in the inbox (incoming grants).
    ///
    /// Fires the callback once immediately with the current snapshot
    /// (which has `loaded = false` and empty entries if the keeper
    /// hasn't been bootstrapped yet), and again on every change.
    ///
    /// The keeper is populated by:
    ///   - `listInbox()` (bootstrap — replaces the entry set)
    ///   - SSE `grant:upsert` / `grant:delete` events (incremental)
    #[wasm_bindgen(js_name = watchInbox)]
    pub async fn watch_inbox(
        &self,
        callback: js_sys::Function,
    ) -> Result<WasmInboxWatcher, JsError> {
        let cb = js_inbox_watcher_callback(callback);
        let mut keeper = self.inbox_keeper.lock().await;
        let handle = keeper.install_watcher(cb);
        Ok(WasmInboxWatcher {
            inbox_keeper: Rc::clone(&self.inbox_keeper),
            handle,
            closed: Rc::new(std::cell::Cell::new(false)),
        })
    }

    /// Subscribe to `chain:forked` SSE events.
    ///
    /// Unlike `watchWorkspaces` / `watchInbox`, this watcher receives
    /// transient signal events — there's no replay of "current state" on
    /// install (forks have no persistent state to project). The callback
    /// fires once per `chain:forked` event the consumer observes, with
    /// the full event payload shape from `SseChainForked`.
    ///
    /// React's `useTreeMutation` consumes this to retry mutations that
    /// raced with a concurrent supersede.
    #[wasm_bindgen(js_name = watchChainForks)]
    pub async fn watch_chain_forks(
        &self,
        callback: js_sys::Function,
    ) -> Result<WasmChainForkWatcher, JsError> {
        let cb = js_chain_fork_watcher_callback(callback);
        let mut keeper = self.chain_fork_keeper.lock().await;
        let handle = keeper.install_watcher(cb);
        Ok(WasmChainForkWatcher {
            chain_fork_keeper: Rc::clone(&self.chain_fork_keeper),
            handle,
            closed: Rc::new(std::cell::Cell::new(false)),
        })
    }
}

// ---------------------------------------------------------------------------
// WasmFileManagerHandle::watchDirectory
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_class = FileManager)]
impl WasmFileManagerHandle {
    /// Subscribe to live changes for a specific directory.
    ///
    /// Fires the callback once with the current snapshot on registration
    /// (if the context has already been loaded), and again on every SSE
    /// event that affects the directory.
    ///
    /// The callback receives `DirectoryTreeSnapshot | null`. A `null`
    /// snapshot means the watched directory has been deleted and the
    /// watcher has been auto-closed — no need to call `.close()` in that
    /// case.
    ///
    /// Returns a `DirectoryWatcher` handle. Call `.close()` to unsubscribe
    /// (typically from a React useEffect cleanup).
    #[wasm_bindgen(js_name = watchDirectory)]
    pub async fn watch_directory(
        &self,
        directory_uri: String,
        callback: js_sys::Function,
    ) -> Result<WasmDirectoryWatcher, JsError> {
        // Ensure the tree is loaded + installed before registering.
        self.ensure_tree_installed().await?;

        let mut keeper = self.tree_keeper.lock().await;
        let cb = js_watcher_callback(callback);

        // Pick scope based on the FileManager's context.
        let handle = match &self.context {
            opake_core::manager::FileContext::Cabinet(_) => {
                keeper.watch_cabinet(directory_uri, cb)
            }
            opake_core::manager::FileContext::Workspace(ws) => {
                keeper.watch_workspace(ws.uri.clone(), directory_uri, cb)
            }
        };

        Ok(WasmDirectoryWatcher {
            tree_keeper: Rc::clone(&self.tree_keeper),
            handle,
            closed: Rc::new(std::cell::Cell::new(false)),
        })
    }

    /// Internal: load the tree via the existing FileManager::load_tree
    /// path and install it in the TreeKeeper if not already present.
    /// This bridges the "rebuild on every load" model with the new
    /// "persistent tree" model until FileManager is fully migrated.
    async fn ensure_tree_installed(&self) -> Result<(), JsError> {
        // Fast path: already installed? Check without holding the opake lock.
        {
            let keeper = self.tree_keeper.lock().await;
            let already = match &self.context {
                opake_core::manager::FileContext::Cabinet(_) => {
                    keeper.cabinet_tree().is_some()
                }
                opake_core::manager::FileContext::Workspace(ws) => {
                    keeper.workspace_tree(&ws.uri).is_some()
                }
            };
            if already {
                return Ok(());
            }
        }

        // Slow path: load the tree via the existing FileManager path, then install.
        let (tree, scope) = {
            let mut guard = self.opake.lock().await;
            let context = &self.context;

            let mut mgr = guard.file_manager(context);
            let tree = mgr.load_tree().await.map_err(wasm_err)?;

            // Extract the scope info before dropping mgr + guard.
            let scope = match context {
                opake_core::manager::FileContext::Cabinet(_) => TreeInstall::Cabinet,
                opake_core::manager::FileContext::Workspace(ws) => TreeInstall::Workspace(
                    ws.uri.clone(),
                    ws.key.clone(),
                    ws.rotation,
                    ws.historical_keys.clone(),
                ),
            };
            (tree, scope)
        };

        // Now install. We need both halves of the cabinet's hybrid private key.
        match scope {
            TreeInstall::Cabinet => {
                let guard = self.opake.lock().await;
                let identity = guard.identity();
                let x25519_private_key =
                    *identity.x25519_private_key_bytes().map_err(wasm_err)?;
                let ml_kem_private_key =
                    *identity.ml_kem_private_key_bytes().map_err(wasm_err)?;
                drop(guard);

                let mut keeper = self.tree_keeper.lock().await;
                keeper.install_cabinet_tree(tree, x25519_private_key, ml_kem_private_key);
            }
            TreeInstall::Workspace(uri, key, rotation, historical) => {
                let mut keeper = self.tree_keeper.lock().await;
                keeper.install_workspace_tree(uri, tree, key, rotation, historical);
            }
        }

        Ok(())
    }
}

enum TreeInstall {
    Cabinet,
    Workspace(
        String,
        opake_core::crypto::ContentKey,
        u64,
        Vec<opake_core::workspace::HistoricalKey>,
    ),
}

// ---------------------------------------------------------------------------
// WasmOpakeHandle::startSseConsumer / stopSseConsumer
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_class = OpakeContext)]
impl WasmOpakeHandle {
    /// Start the SSE event consumer. Spawns a background task that
    /// connects to the indexer's `/api/events` endpoint, pulls events,
    /// and dispatches them to the shared TreeKeeper.
    ///
    /// `indexer_url` is optional: if omitted, the URL is resolved from
    /// the Opake instance's priority chain (runtime override, PDS
    /// accountConfig, compile-time default). If provided, it's promoted
    /// to the runtime override (priority 1) for this Opake instance —
    /// it wins over accountConfig and persists across subsequent indexer
    /// calls within the same session.
    ///
    /// Idempotent: subsequent calls are no-ops while an existing
    /// consumer is running. React StrictMode's double-mount is thus
    /// harmless — only one consumer task exists per OpakeContext.
    #[wasm_bindgen(js_name = startSseConsumer)]
    pub async fn start_sse_consumer(&self, indexer_url: Option<String>) -> Result<(), JsError> {
        // If the caller supplied a URL, promote it to the runtime override
        // (priority 1) before resolving — so passing a URL here wins over
        // PDS accountConfig and is consistent with every subsequent
        // indexer call made through this Opake instance. Resolve BEFORE
        // flipping `sse_started` so a URL-less call on a fresh Opake
        // without a default still surfaces the error cleanly.
        let resolved_url = {
            let mut guard = self.inner.lock().await;
            if let Some(url) = indexer_url {
                guard.set_indexer_url(url);
            }
            guard.resolve_indexer_url()
        };

        if self.sse_started.get() {
            log::debug!("[sse] consumer already running, ignoring startSseConsumer");
            return Ok(());
        }
        self.sse_started.set(true);

        let opake_rc = Rc::clone(&self.inner);
        let tree_keeper_rc = Rc::clone(&self.tree_keeper);
        let workspace_keeper_rc = Rc::clone(&self.workspace_keeper);
        let inbox_keeper_rc = Rc::clone(&self.inbox_keeper);
        let chain_fork_keeper_rc = Rc::clone(&self.chain_fork_keeper);
        let started_flag = Rc::clone(&self.sse_started);

        let token_fetcher = make_token_fetcher(Rc::clone(&opake_rc), resolved_url.clone());
        let sleep_fn: SleepFn = Box::new(|d| Box::pin(wasm_sleep(d)));
        let jitter_fn: JitterRng = Box::new(|| js_sys::Math::random());

        let mut consumer = SseConsumer::new(
            WasmSseTransport::new(),
            resolved_url,
            token_fetcher,
            sleep_fn,
            jitter_fn,
        );

        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let event = match consumer.next_event().await {
                    Ok(e) => e,
                    Err(e) => {
                        log::warn!("[sse] consumer terminated: {e}");
                        break;
                    }
                };

                // `stop_sse_consumer` signals termination by clearing
                // the started flag. Check after every await so we
                // don't apply one last event after the owning
                // component unmounted.
                if !started_flag.get() {
                    log::debug!("[sse] consumer stopped, exiting");
                    break;
                }

                {
                    let mut keeper = tree_keeper_rc.lock().await;
                    // Re-check the flag after acquiring the lock: if the
                    // consumer was stopped while we were waiting for it,
                    // bail instead of re-inhabiting the tree the wipe
                    // task is about to drain (or just drained).
                    if !started_flag.get() {
                        log::debug!("[sse] consumer stopped while awaiting tree_keeper lock");
                        break;
                    }
                    if let Err(e) = keeper.apply_event(&event) {
                        log::warn!("[sse] tree_keeper apply failed: {e}");
                    }
                }

                // Workspace list updates: apply directly to the keeper
                // so subscribers see changes without an indexer round-
                // trip. Idempotent upserts (same rotation + same data)
                // don't re-fire watchers — see `WorkspaceKeeper::upsert`.
                //
                // Same post-lock flag recheck as the tree apply above:
                // the helper bails internally if a stop landed while it
                // was waiting for the workspace_keeper / inbox_keeper
                // mutex.
                apply_keyring_to_workspace_keeper(
                    &opake_rc,
                    &workspace_keeper_rc,
                    &started_flag,
                    &event,
                )
                .await;

                // Grant events: apply to the inbox keeper so the
                // "Shared with me" view updates live.
                apply_grant_to_inbox_keeper(&opake_rc, &inbox_keeper_rc, &started_flag, &event)
                    .await;

                // Chain-fork dispatch: fan out to any registered
                // mutation-retry subscribers. The keeper holds no state,
                // so each event is purely transient.
                if let SseEvent::ChainForked(ref fork) = event {
                    let mut keeper = chain_fork_keeper_rc.lock().await;
                    if !started_flag.get() {
                        break;
                    }
                    keeper.dispatch(fork);
                }
            }
            // Task exited — clear the flag in case we broke on a
            // transport error rather than an explicit stop, so a
            // future start spawns a fresh consumer.
            started_flag.set(false);
            drop(opake_rc);
        });

        Ok(())
    }

    /// Stop the SSE consumer. Only flips the `sse_started` flag and
    /// clears the proposal-sync debounce state — the consumer loop
    /// terminates on its next `next_event().await`. Tree + workspace
    /// caches are intentionally preserved: stopping the stream doesn't
    /// mean the user is signing out, only that no new events will be
    /// applied. Call `wipeState()` separately when crypto material
    /// should be zeroed (logout, account switch).
    #[wasm_bindgen(js_name = stopSseConsumer)]
    pub fn stop_sse_consumer(&self) {
        self.sse_started.set(false);
    }

    /// Drain every in-memory keeper: directory trees, the workspace
    /// list, the inbox. Drops cached `ContentKey`s (triggering their
    /// `ZeroizeOnDrop`) and cached decrypted directory names.
    ///
    /// Synchronous from JS so callers in React `useEffect` cleanup can
    /// invoke it directly. The async keeper locks are awaited on a
    /// `spawn_local` task — fire-and-forget is safe because no caller
    /// observes mid-wipe state.
    ///
    /// Typical sequence at logout is `stopSseConsumer()` then
    /// `wipeState()`. OpakeProvider's unmount effect does this pair.
    #[wasm_bindgen(js_name = wipeState)]
    pub fn wipe_state(&self) {
        let tree_keeper = Rc::clone(&self.tree_keeper);
        let workspace_keeper = Rc::clone(&self.workspace_keeper);
        let inbox_keeper = Rc::clone(&self.inbox_keeper);
        let chain_fork_keeper = Rc::clone(&self.chain_fork_keeper);
        wasm_bindgen_futures::spawn_local(async move {
            let mut tk = tree_keeper.lock().await;
            tk.uninstall_all();
            drop(tk);
            let mut wk = workspace_keeper.lock().await;
            wk.uninstall_all();
            drop(wk);
            let mut ik = inbox_keeper.lock().await;
            ik.uninstall_all();
            drop(ik);
            let mut cfk = chain_fork_keeper.lock().await;
            cfk.uninstall_all();
            log::debug!(
                "[sse] tree_keeper + workspace_keeper + inbox_keeper + chain_fork_keeper drained on wipeState"
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a token fetcher closure that uses the shared Opake to request
/// a fresh SSE token on every connect attempt.
fn make_token_fetcher(opake_rc: Rc<Mutex<WasmOpake>>, indexer_url: String) -> TokenFetcher {
    Box::new(move || {
        let opake_rc = Rc::clone(&opake_rc);
        let indexer_url = indexer_url.clone();
        Box::pin(async move {
            let guard = opake_rc.lock().await;
            let did = guard.did().to_string();
            let signing_key = guard
                .identity()
                .signing_key_bytes()
                .map_err(|e| opake_core::error::Error::Sse(format!("{e}")))?
                .ok_or_else(|| {
                    opake_core::error::Error::Sse("identity has no signing key".into())
                })?;
            let transport = opake_core::client::WasmTransport::new();
            request_sse_token(&transport, &indexer_url, &did, &signing_key).await
        })
    })
}

/// Promise-based sleep using JS `setTimeout`. Works in any context with
/// a global `setTimeout` (Window, Worker). Used by `SseConsumer` for
/// exponential-backoff reconnect timing.
async fn wasm_sleep(duration: Duration) {
    let ms = duration.as_millis() as i32;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        let set_timeout = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout"))
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
        if let Some(f) = set_timeout {
            let _ = f.call2(&global, &resolve, &JsValue::from_f64(ms as f64));
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Apply a keyring record event to the `WorkspaceKeeper`.
///
/// `KeyringUpsert`: build an entry from the record using the caller's
/// identity. `Some` → upsert; `None` (DID absent from member list,
/// i.e. we were rotated out) → delete. `KeyringDelete`: delete by URI.
/// Other events are no-ops.
///
/// Acquires the opake lock **first** (for identity), then drops it
/// before acquiring the workspace_keeper lock. Both callers — this
/// function and `listWorkspaces` — release opake before taking keeper,
/// so there is no concurrent double-holding and no deadlock risk.
/// The real reason for the release order: the identity private key
/// used to build the entry doesn't need to be held across the keeper
/// apply, and holding both mutexes longer than necessary reduces SSE
/// throughput. A narrow race window exists where a concurrent
/// `listWorkspaces` bootstrap can land between our opake release and
/// keeper acquire; this is benign — the next SSE event or the keeper's
/// idempotent upsert self-corrects.
async fn apply_keyring_to_workspace_keeper(
    opake_rc: &Rc<Mutex<WasmOpake>>,
    workspace_keeper_rc: &Rc<Mutex<WorkspaceKeeper>>,
    started_flag: &Rc<std::cell::Cell<bool>>,
    event: &SseEvent,
) {
    match event {
        SseEvent::KeyringUpsert(envelope) => {
            let maybe_entry = {
                let guard = opake_rc.lock().await;
                let did = guard.did().to_string();
                let private_keys = match guard.identity().owned_private_keys() {
                    Ok(pk) => pk,
                    Err(e) => {
                        log::warn!("[sse] workspace upsert: owned_private_keys failed: {e}");
                        return;
                    }
                };
                wk::try_build_entry(envelope, &did, &private_keys.bundle())
            };
            let mut keeper = workspace_keeper_rc.lock().await;
            if !started_flag.get() {
                return;
            }
            keeper.apply_keyring_record(&envelope.uri, maybe_entry);
        }
        SseEvent::KeyringDelete(payload) => {
            let mut keeper = workspace_keeper_rc.lock().await;
            if !started_flag.get() {
                return;
            }
            keeper.delete(&payload.uri);
        }
        _ => {}
    }
}

/// Apply a grant record event to the `InboxKeeper`.
///
/// `GrantUpsert`: build an entry filtered by the caller's DID.
/// `GrantDelete`: delete by URI. Other events are no-ops.
async fn apply_grant_to_inbox_keeper(
    opake_rc: &Rc<Mutex<WasmOpake>>,
    inbox_keeper_rc: &Rc<Mutex<InboxKeeper>>,
    started_flag: &Rc<std::cell::Cell<bool>>,
    event: &SseEvent,
) {
    match event {
        SseEvent::GrantUpsert(envelope) => {
            let my_did = opake_rc.lock().await.did().to_string();
            let Some(entry) = ik::try_build_entry_from_envelope(envelope, &my_did) else {
                return;
            };
            let mut keeper = inbox_keeper_rc.lock().await;
            if !started_flag.get() {
                return;
            }
            keeper.upsert(entry);
        }
        SseEvent::GrantDelete(payload) => {
            let mut keeper = inbox_keeper_rc.lock().await;
            if !started_flag.get() {
                return;
            }
            keeper.delete(&payload.uri);
        }
        _ => {}
    }
}

/// Wrap a JS function as an [`InboxWatcherCallback`] that serializes
/// the snapshot to a JS object on each call.
fn js_inbox_watcher_callback(callback: js_sys::Function) -> InboxWatcherCallback {
    Box::new(move |snapshot: &InboxSnapshot| {
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        let snapshot_js = match snapshot.serialize(&serializer) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[sse] inbox snapshot serialize failed: {e}");
                return;
            }
        };
        if let Err(e) = callback.call1(&JsValue::NULL, &snapshot_js) {
            log::warn!("[sse] inbox watcher callback threw: {e:?}");
        }
    })
}

/// Wrap a JS function as a [`ChainForkWatcherCallback`]. The payload is
/// emitted in snake_case to match how the broadcaster wire format is
/// surfaced elsewhere — Zod schemas on the JS side reshape to camelCase.
fn js_chain_fork_watcher_callback(callback: js_sys::Function) -> ChainForkWatcherCallback {
    Box::new(
        move |event: &opake_core::indexer::sse::events::SseChainForked| {
            let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
            let event_js = match event.serialize(&serializer) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("[sse] chain-fork event serialize failed: {e}");
                    return;
                }
            };
            if let Err(e) = callback.call1(&JsValue::NULL, &event_js) {
                log::warn!("[sse] chain-fork watcher callback threw: {e:?}");
            }
        },
    )
}

/// Wrap a JS function as a [`WorkspaceWatcherCallback`] that serializes
/// the snapshot to a JS object on each call.
fn js_workspace_watcher_callback(callback: js_sys::Function) -> WorkspaceWatcherCallback {
    Box::new(move |snapshot: &WorkspaceSnapshot| {
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        let snapshot_js = match snapshot.serialize(&serializer) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[sse] workspace snapshot serialize failed: {e}");
                return;
            }
        };
        if let Err(e) = callback.call1(&JsValue::NULL, &snapshot_js) {
            log::warn!("[sse] workspace watcher callback threw: {e:?}");
        }
    })
}

/// Wrap a JS function as a `WatcherCallback` that builds a snapshot on
/// each notification and serializes it to a JS object.
fn js_watcher_callback(callback: js_sys::Function) -> WatcherCallback {
    Box::new(move |tree: Option<&DirectoryTree>| {
        let snapshot_js = match tree {
            Some(t) => {
                let snapshot = build_snapshot(t);
                let serializer =
                    serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
                match snapshot.serialize(&serializer) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[sse] snapshot serialize failed: {e}");
                        return;
                    }
                }
            }
            None => JsValue::NULL,
        };
        // Invoke the JS callback. Errors propagate as JS exceptions into
        // the WASM task — log but don't panic (one broken React component
        // shouldn't crash the event loop).
        if let Err(e) = callback.call1(&JsValue::NULL, &snapshot_js) {
            log::warn!("[sse] watcher callback threw: {e:?}");
        }
    })
}
