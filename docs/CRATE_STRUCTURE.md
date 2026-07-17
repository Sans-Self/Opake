# Opake — Monorepo Structure

This is the annotated file tree. Larger test suites are extracted to sibling
`*_tests.rs` files via `#[cfg(test)] #[path = "..._tests.rs"] mod tests;`; those
siblings are omitted below to keep the map focused on behavior.

```
crates/
  opake-core/          Platform-agnostic protocol library (compiles to WASM)
    src/
      opake.rs         Opake<T, R, S> root context — owns storage, factory for FileManager/WorkspaceAdmin. All CLI commands (except pair request and recover) route through here. Workspace CRUD, sharing, identity, pairing, maintenance. signoff() auto-persists session
      cabinet.rs       Cabinet domain type (personal file space). ZeroizeOnDrop
      workspace.rs     Workspace domain type (shared file space, wraps keyring data). Rotation-aware group_keys(). ZeroizeOnDrop
      rewrap.rs        Re-wrap sweep — migrate document content-key wraps from a historical group key to the current one after a rotation (background hygiene)
      resolve.rs       Handle/DID → PDS → public key resolution pipeline
      scope.rs         OAuth scope registry — OPAKE_COLLECTIONS (single source of truth for all at.opake.* collections) + oauth_scope() builder
      tid.rs           TID (Timestamp ID) generator for client-side AT Protocol rkeys
      timestamp.rs     RFC 3339 formatter from microseconds — single clock source for core (no chrono dep)
      atproto.rs       AT-URI parsing, shared AT Protocol primitives
      account_config.rs  Fetch/publish singleton account config from PDS
      storage.rs       Config, Identity types + Storage trait (cross-platform contract)
      paths.rs         Data directory resolution (env, XDG, fallback)
      error.rs         Typed error hierarchy (thiserror)
      test_utils.rs    MockTransport + response queue (behind test-utils feature)
      indexer/
        mod.rs         Indexer client, types, submodule declarations
        auth.rs        Opake-Ed25519 header construction for indexer calls
        client.rs      HTTP wrappers for /api/inbox, /api/keyrings, /api/workspace, /api/cabinet, /api/events/token, /api/workspace/chain-head
        daemon.rs      Indexer-coordinated maintenance tasks (subset of the core daemon registry)
        types.rs       InboxGrant, KeyringEntry, WorkspaceEntry, etc.
        retry.rs       Bounded retry-with-backoff at the indexer-resolution boundary — covers the unbounded PDS→firehose→indexer→snapshot propagation gap
        sse/
          mod.rs             SseEvent enum + submodule declarations
          consumer.rs        Reconnecting SSE consumer
          events.rs          Event payload types
          parser.rs          text/event-stream frame parsing
          reconnect.rs       Reconnect/backoff policy
          transport.rs       Connection trait (wasm + native)
          reqwest_connection.rs / wasm_connection.rs  Platform transports
          mock.rs            In-memory transport for tests
        tree_keeper/mod.rs       Per-DID in-memory directory tree. Bootstrapped from the cabinet/workspace snapshot, patched by document/directory SSE events. watchDirectory installs snapshot callbacks
        workspace_keeper/mod.rs  In-memory workspace list. Bootstrapped by listWorkspaces, patched by keyring:upsert / keyring:delete events. watchWorkspaces installs callbacks
        inbox_keeper/mod.rs      In-memory incoming-share list. Bootstrapped by listInbox, patched by grant:upsert / grant:delete events (indexer fans both to owner and recipient)
        chain_fork_keeper.rs     Stateless pub-sub for chain:forked SSE signals — no retained state, callers react and retry
      manager/
        mod.rs         FileManager<'a, T, R, S> (borrows &mut Opake + &FileContext)
        types.rs       UploadRequest, UploadResult, DownloadResult, MutationOutcome, FileContext
        upload.rs      upload_at — path-based atomic upload (encrypt + blob + document + directory entry via applyWrites)
        download.rs    download_at — cabinet/workspace/cross-PDS download dispatch
        delete.rs      Atomic delete (applyWrites), delete_recursive (tree-walking)
        move_entry.rs  Atomic move (source removal + target addition)
        rename.rs      rename_directory (re-encrypt metadata with new name)
        directory.rs   ensure_root, create_directory_at, delete_directory
        editor.rs      read_metadata, update_metadata, update_content, fetch_content_key
        substitute.rs  Curatorial substitute-and-cascade — swap a directory entry and propagate CID/URI changes up to the root
        sharing.rs     share, revoke_share, list_shares (cabinet-only)
        tree.rs        load_tree, resolve_entry, resolve_document_names, resolve_document_metadata_in
        admin.rs       WorkspaceAdmin<T, R, S> — add_member, remove_member, leave
      records/
        mod.rs         Versioned trait, check_version(), re-exports (SCHEMA_VERSION comes from opake-crypto)
        classify.rs    Record classification — the shared read-surface contract (Understood / NeedsNewerClient / Corrupt). Governs record-validity
        vocabulary.rs  Version-pinned registry vocabulary, loaded from lexicons/vocabulary.json (shared with the indexer)
        defs.rs        EncryptionEnvelope, KeyringRef, Role, KeyringMember, KeyWrapping, DirectKeyWrapping, KeyringKeyWrapping
        document.rs    DirectEncryption, KeyringEncryption, Encryption, Document
        directory.rs   Directory (carries optional workspaceId + lineage; isWorkspaceRoot flag on the root chain)
        keyring.rs     KeyHistoryEntry, Keyring (owner field, lineage = workspace identity)
        grant.rs       Grant
        public_key.rs  PublicKeyRecord, collection/rkey constants
        pending_share.rs    PendingShare (PDS-backed queue of in-flight shares to not-yet-ready DIDs)
        account_config.rs   AccountConfig (singleton; proof-of-life heartbeat)
        pair_request.rs / pair_response.rs  Device-pairing records
      client/
        mod.rs         Re-exports
        transport.rs   Transport trait (HTTP abstraction for WASM compat)
        reqwest_transport.rs / wasm_transport.rs  Platform transports
        did.rs         Unauthenticated DID resolution and cross-PDS queries
        dns.rs         DNS-based handle resolution
        list.rs        Generic paginated collection fetcher
        time.rs        Injected clock abstraction
        session_refresh.rs  Token-refresh scheduling logic
        dpop.rs        DPoP keypair (P-256/ES256) + proof JWT generation
        oauth_discovery.rs  OAuth AS discovery + PKCE S256; AuthorizationServerMetadata::par_endpoint()
        oauth_token.rs PAR, authorization code exchange, token refresh (all DPoP-bound), build_client_id
        xrpc/
          mod.rs       XrpcClient, Session enum (Legacy/OAuth), dual auth dispatch
          auth.rs      login(), refresh_session()
          blobs.rs     upload_blob(), get_blob()
          repo.rs      create_record/put_record/get_record/list_records/delete_record/apply_writes. ApplyWriteOp::Create carries an optional rkey for client-generated TIDs
      directories/
        mod.rs         Re-exports, constants, envelope encryption helpers
        create.rs      create_directory()
        delete.rs      delete_directory() — single empty directory
        entries.rs     prepare_add_entry / prepare_remove_entry — return ApplyWriteOp for batching
        get_or_create_root.rs  Root singleton (cabinet rkey "self"); the workspace root is a flag-marked chain (isWorkspaceRoot, client TID rkeys, forward-walked from genesis)
        chain.rs       Walk supersede chains across PDSes (directory + keyring back-edges)
        cascade.rs     Execute a directory supersede cascade — propagate a child mutation up to the workspace/cabinet root
        move_entry.rs  move_entry(), check_cycle()
        remove.rs      Path-aware recursive deletion with parent cleanup
        tree.rs        DirectoryTree — in-memory snapshot, decrypt_names_with_group_keys, resolution helpers
        list.rs        list_directories()
      documents/
        mod.rs         Re-exports, shared test fixtures (mock_client, dummy_document)
        upload.rs      prepare_upload / prepare_upload_keyring — JSON for applyWrites batching
        download.rs    download() — direct-encrypted documents
        download_grant.rs   download_shared() — cross-PDS via grant URI
        download_keyring.rs download_keyring_document(), download_with_group_key(), fetch_content_key_with_group_key()
        update.rs      In-place content/metadata update
        delete.rs      delete_document()
      metadata/
        mod.rs         Re-exports
        read.rs        Fetch document record + decrypt metadata
        write.rs       Mutate metadata + re-encrypt + putRecord
      keyrings/
        mod.rs         Re-exports, DidMember, resolve_keyring_uri()
        create.rs      create_keyring() → group key + record
        list.rs        list_keyrings()
        add_member.rs  add_member(), AddMemberParams
        remove_member.rs  remove_member() — rotate group key, re-wrap to remaining members
      sharing/
        mod.rs         Re-exports
        create.rs      create_grant()
        list.rs        list_grants()
        revoke.rs      revoke_grant()
        heal.rs        Detect grants wrapped to a recipient's rotated-out key; deletes stale grants
        pending.rs     Pending-share queue — create, list, retry, cancel
      pairing/
        mod.rs         Re-exports
        request.rs     create_pair_request() — write ephemeral key to PDS, persist privkey via Storage
        respond.rs     respond_to_pair_request() — encrypt + wrap identity (existing device)
        receive.rs     try_complete_pair() — poll, decrypt, save Identity, tear down pair state
        cancel.rs      cancel_pair_request() — wipe pair state on user back-out
        cleanup.rs     cleanup_pair_records() — daemon sweep for expired/orphan records

  opake-crypto/        Client-side cryptographic primitives (no I/O, platform-agnostic)
    src/
      lib.rs           Re-exports, RNG type aliases, hybrid-KEM constants (HYBRID_WRAP_ALGO, ML_KEM_*, AES_GCM_NONCE_LEN, …), SCHEMA_VERSION, key-bundle types, generate_ephemeral_keypair
      content.rs       AES-256-GCM: generate_content_key(), encrypt_blob(), decrypt_blob()
      key_wrapping.rs  Hybrid X25519 + ML-KEM-768 KEM (x25519-mlkem768-hkdf-a256kw-v2): wrap_key(), unwrap_key(), create_group_key(). HKDF transcript commits to both static pubkeys, the ephemeral pubkey, and the ML-KEM ciphertext (splice resistance)
      keyring_wrapping.rs  Symmetric AES-KW: wrap/unwrap content key under a group key
      metadata.rs      encrypt_metadata(), decrypt_metadata(); DocumentMetadata, KeyringMetadata, GrantMetadata, DirectoryMetadata
      seal_context.rs  AAD context for content/metadata sealing — binds every AES-256-GCM ciphertext to the record's lineage anchor and type
      transcript.rs    Injective context-transcript encoding shared by the HKDF wrap info and the seal AAD
      identity_tag.rs  Workspace identity-tag derivation — the genesis keyring rkey derived from the rotation-0 group key + owner DID (HKDF → Ed25519 pubkey → base32(SHA-256[..16]))
      secrets.rs       Raw hybrid key material (X25519 + Ed25519 + ML-KEM-768); one struct from both random-generation and mnemonic paths. Zeroize on drop
      wire.rs          WrappedKey + EncryptedMetadata — the literal output shapes of wrap_key() and encrypt_metadata()
      at_bytes.rs      AtBytes — atproto's { "$bytes": <base64> } wrapper for every byte payload on the wire
      error.rs         Error: Encryption/Decryption/KeyWrap/Mnemonic/InvalidEncoding (mapped onto opake-core's Error)
      mnemonic/
        mod.rs         Mnemonic type (Zeroize), parse_mnemonic(), checksum validation
        generate.rs    generate_mnemonic() — 256-bit entropy → 24 words
        derive.rs      derive_keys_from_mnemonic() — PBKDF2-HMAC-SHA512 → HKDF paths → X25519/Ed25519/ML-KEM secrets (zeroize on drop)
        format.rs      format_mnemonic_grid(), parse_mnemonic_grid() — .txt import/export
      bip39_english.txt  Embedded BIP-39 wordlist

  opake-wasm/          WASM bridge (wasm-pack, wasm_bindgen)
    src/
      lib.rs           Module declarations, WASM init, pure crypto + tree exports (stateless)
      auth_wasm.rs     OAuth login exports: startOAuthLogin, completeOAuthLogin, loginWithAppPasswordWasm. All token handling in WASM
      opake_wasm.rs    WasmOpakeHandle (JS: OpakeContext). Exports tokenExpiresAt, proactiveRefresh, checkSession, wipeState, getDid, plus maintenance ops
      file_manager_wasm.rs  WasmFileManagerHandle (JS: FileManager). File operations within a cabinet or workspace context
      sse_wasm.rs      SSE consumer bindings: startSseConsumer, stopSseConsumer, watchWorkspaces, watchInbox
      pair_wasm.rs     New-device pair bindings (pre-identity; take JsStorage + DID directly). No key material crosses the boundary
      bootstrap_gate.rs  Snapshot/stream sequencing — orders the listWorkspaces/listInbox snapshot ahead of live SSE patches
      bindings.rs      Cross-boundary DTOs with ts_rs TypeScript declarations
      js_storage.rs    JsStorage — Storage impl that calls back into a JS IndexedDbStorage adapter
      daemon.rs        Daemon task registry bindings + shared interval/TTL constants
      wasm_util.rs     make_opake_from_storage, cabinet_context helpers; error/marshalling utilities. WasmOpake = Opake<WasmTransport, OsRng, JsStorage>

  opake-derive/        Proc-macro crate
    src/
      lib.rs           #[derive(RedactedDebug)] — Debug + Zeroize + Drop for structs with #[redact] fields (emits ::opake_crypto::Redacted paths). #[signoff] / #[signoff(self)] — attribute macro for automatic session persistence

apps/
  cli/                 CLI binary wrapping opake-core (package: opake-cli)
    src/
      main.rs          Clap app, command dispatch
      config.rs        FileStorage (impl Storage for filesystem), anyhow wrappers
      session.rs       CommandContext, build_opake() (single CLI construction path, shared with the daemon)
      identity.rs      Identity loading, migration, permission checks
      keyring_store.rs Local group key persistence (per-keyring)
      oauth.rs         OAuth loopback redirect server + browser open
      prompt.rs        Confirmation prompts (stdin, stderr UX)
      utils.rs         Test harness, env helpers
      commands/
        mod.rs         Execute trait, module re-exports
        account.rs     Account subcommand group (login, logout, list, set-default, session)
        login.rs       Auth + seed phrase generation + key publish (OAuth-first)
        recover.rs     Seed phrase recovery (stdin or --file .txt import)
        upload.rs      upload_at — direct or --workspace, path-based
        download.rs    download_at — cabinet, --workspace, --grant, --stdout
        cat.rs         Alias for download --stdout
        ls.rs          Directory-aware listing (--workspace, --long, --tag, optional path)
        metadata.rs    View/edit document metadata (rename, description)
        mkdir.rs       create_directory_at — personal or --workspace, path-based
        tree.rs        Unified cabinet/workspace tree with --workspace
        rm.rs          delete_recursive via FileManager
        move_cmd.rs    FileManager move_entry
        resolve.rs     Identity resolution display
        share.rs / share_group.rs / revoke.rs / shared.rs / inbox.rs  Sharing (grant create, revoke, list created, list received)
        workspace.rs   Workspace CRUD (create, ls, add-member, leave); remove-member via WorkspaceAdmin
        pair.rs        Device pairing (request, approve)
        session_cmd.rs Session token management (inspect, refresh)
        config.rs      View/edit account config
        completions.rs Shell completion script generation
        purge.rs       Delete all records from PDS (danger zone)
        daemon/        Background daemon (session refresh, pair cleanup, grant healing, share retry)

  web/                 React SPA (Vite + TanStack Router + Tailwind + daisyUI)
    src/
      client.tsx       SPA entry point
      router.tsx       TanStack Router config
      lib/             AT-URI helpers, tree traversal, formatters, encoding/fingerprint, file context, seed-phrase parsing, profile resolution, sharing/workspace form helpers
      stores/          Zustand: auth (identity state machine), app (loading), tasks (daemon status), toast
      routes/          File-based routing: __root, _public (landing/docs/FAQ), cabinet/ + devices/ (lazy, auth-guarded)
      components/
        cabinet/       File browser, sidebar, editor, workspace UI, dialogs
        devices/       Identity setup, seed phrase, pairing, conflict resolution
        content/       MDX rendering, landing page sections
    content/docs/      MDX handbook mirrored from the repo docs (build/, understand/, use/, faq, index)

    WASM lives in packages/opake-sdk/wasm/ (built by `just wasm`) and is imported via
    @opake/sdk. No web-local worker layer: all file/identity operations run through
    @opake/sdk and @opake/react hooks on the main thread.

  indexer/             Elixir/Phoenix indexer + REST API + SSE broadcaster
    lib/
      opake_indexer/
        application.ex       OTP supervision tree (Repo, KeyCache, Endpoint, Jetstream consumer, SSE ETS tables)
        firehose.ex          Dispatches parsed Jetstream events to the records / chain_heads tables; cursor saving
        firehose/            Runtime state, heartbeat, consume-lag tracking
        authority.ex         Authority validation for federation supersedes (manager / additive-editor / self-removal rules)
        backfill.ex          Backfill a DID's at.opake.* records when the cursor is absent or stale
        tombstone_cleanup.ex Hourly purge of soft-deleted rows older than the retention window
        release.ex           Release tasks (create_db, migrate, rollback, status)
        repo.ex              Ecto Repo
        auth/
          plug.ex            Opake-Ed25519 header verification (Plug)
          key_cache.ex       GenServer + ETS, 5-min TTL per DID
          key_fetcher.ex / key_fetcher_behaviour.ex  DID → PDS → publicKey → signingKey resolution
          base64.ex          Flexible base64 decode (padded/unpadded)
        jetstream/
          consumer.ex        WebSockex client with exponential backoff
          event.ex           Jetstream JSON → tagged tuples
          compression.ex     Per-consumer zstd streaming context
        lexicon/
          schema.ex          Minimal structural Lexicon validator over at.opake.* records
          validator.ex       Ingest gate — refuses malformed / off-vocabulary records at the network edge
          vocabulary.ex      Version-pinned registry vocabulary (mirror of lexicons/vocabulary.json)
        sse/
          broadcaster.ex     PubSub fan-out for every indexed event
          topics.ex          Topic builders (personal, workspace)
          token_store.ex     Single-use opaque tokens for /api/events
          connection_tracker.ex  ETS-backed per-DID SSE connection cap (5)
        queries/
          record_queries.ex      Record CRUD, membership resolution, snapshot/sync, inbox pagination
          chain_head_queries.ex  Chain-head upsert/load per (workspace_id, kind)
          cursor_queries.ex      Singleton cursor upsert/load
          pagination.ex          Shared cursor-based pagination helpers
        schemas/
          record.ex          One row per AT-URI: collection, author_did, workspace_id, supersedes_uri, is_workspace_root, cid, indexed_at, updated_at, deleted_at, record_jsonb
          chain_head.ex      Current head of a tracked chain, PK (workspace_id, kind ∈ {keyring, workspace_root})
          cursor.ex          Singleton Jetstream cursor (id=1)
      opake_indexer_web/
        router.ex             /api/health (public); everything else auth'd: inbox, keyrings, workspace/{snapshot,sync,chain-head}, cabinet/{snapshot,sync}, events + events/token
        endpoint.ex           Bandit HTTP, long SSE read timeouts
        plugs/rate_limit.ex   Hammer ETS rate limiting per IP (skipped for /api/events)
        plugs/cors.ex         CORS origin enforcement
        controllers/          health, inbox, keyrings, workspace, cabinet, events, plus pagination/tree shaping helpers
      mix/tasks/
        opake.resync.ex       Backfill opake records from a PDS into the indexer
        opake.tail.ex         Tail the Jetstream firehose, print every frame

packages/
  opake-sdk/             @opake/sdk — TypeScript SDK wrapping the WASM bindings
    src/
      index.ts           Package entry point, re-exports
      opake.ts           Opake client (auth, identity, workspaces, SSE, daemon ops)
      file-manager.ts    FileManager (upload, download, tree, metadata, directory CRUD)
      auth.ts            OAuth/DPoP two-step login + app password flows
      pairing.ts         Device pairing — new-device statics + existing-device helpers
      schemas.ts         Zod schemas for WASM return values (runtime validation at the boundary)
      storage.ts         Storage interface (mirrors the opake-core trait)
      storage-adapter.ts JS-side adapter the Rust JsStorage binds against
      storage/           Dexie IndexedDbStorage + in-memory Storage for tests
      wasm.ts            WASM init + bridge helpers
      finalizer.ts       FinalizationRegistry wrapper for WASM handle disposal
      types.ts           Domain type exports
      errors.ts          Typed error hierarchy
    wasm/                wasm-pack output (imported by the SDK)

  opake-daemon/          @opake/daemon — Background task scheduler (browser main thread)
    src/
      scheduler.ts       startDaemon() — interval-driven task loop
      tasks.ts           Task definitions (session refresh, pair cleanup, grant healing, share retry)
      types.ts           Task type definitions

  opake-react/           @opake/react — React 19 hooks over @opake/sdk
    src/
      provider.tsx       OpakeProvider (FileManagerCache + OptimisticOverlay + SSE lifecycle)
      file-manager-cache.ts   Refcounted per-context FileManager cache
      optimistic-overlay.ts   Per-scope snapshot-transforming patch store
      keys.ts            React Query key factories
      hooks/             use-directory, use-upload/delete/move/download, use-workspaces/inbox/shares,
                         use-*-mutations, use-daemon, bootstrap-once, use-file-manager, use-start-sse-consumer

tests/                   Cross-package integration tests (CLI-driven Rust tests)
```

The boundary is strict: `opake-core` never touches the filesystem, stdin, or any
platform-specific API. All I/O happens through the `Storage` trait — `FileStorage`
(CLI, filesystem) and `IndexedDbStorage` (web, IndexedDB) implement the same contract
with platform-specific backends. This keeps `opake-core` compilable to WASM.
Cryptography lives one layer down in `opake-crypto`, which is equally I/O-free and
shared by both. The `@opake/sdk` package wraps the WASM bindings in a TypeScript API;
the web frontend consumes Opake through the SDK, not raw WASM imports.

The `Opake<T, R, S>` struct is the root context for every operation. A constructed
`Opake` always has an Identity — `for_account` returns `Error::IdentityMissing` for
authenticated accounts without one, and callers route to recovery or pairing to
bootstrap. All CLI commands route through `Opake` except `pair request` and `recover`
(neither has an Identity yet — both use the storage-backed free functions and the raw
XRPC client). Construct one with an authenticated client, identity, and storage, then
call `.file_context(workspace_name?)` to resolve the target, `.file_manager(&context)`
for file operations, or `.workspace_admin()` for membership management. Session
persistence is automatic via `#[signoff]` / `#[signoff(self)]` on every public
mutation. Platform differences — transport, RNG, clock, storage — are injected via type
parameters and function pointers, so the domain layer carries no conditional
compilation.
