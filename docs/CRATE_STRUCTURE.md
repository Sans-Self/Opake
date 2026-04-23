# Opake — Monorepo Structure

```
crates/
  opake-core/          Platform-agnostic library (compiles to WASM)
    src/
      opake.rs         Opake<T, R, S> root context — owns storage, factory for FileManager/WorkspaceAdmin. All CLI commands (except pair request) route through here. Methods for workspace mgmt, sharing, identity, pairing, maintenance, low-level records. signoff() auto-persists session
      cabinet.rs       Cabinet domain type (personal file space). ZeroizeOnDrop
      workspace.rs     Workspace domain type (shared file space, wraps keyring data). ZeroizeOnDrop
      tid.rs           TID (Timestamp ID) generator for client-side AT Protocol rkeys
      timestamp.rs     RFC 3339 formatter from microseconds — single clock source for Opake (no chrono dep in core)
      atproto.rs       AT-URI parsing, shared AT Protocol primitives
      account_config.rs  Fetch/publish singleton account config from PDS
      resolve.rs       Handle/DID → PDS → public key resolution pipeline
      scope.rs         OAuth scope registry — OPAKE_COLLECTIONS (single source of truth for all app.opake.* collections) + oauth_scope() builder
      storage.rs       Config, Identity types + Storage trait (cross-platform contract)
      paths.rs         Data directory resolution (env, XDG, fallback)
      daemon.rs        Background task registry (shared definitions for CLI + web). Daemon builds Opake per account per task iteration, auto-persists via signoff
      error.rs         Typed error hierarchy (thiserror)
      test_utils.rs    MockTransport + response queue (behind test-utils feature)
      indexer/
        mod.rs         Indexer client, types, submodule declarations
        auth.rs        Opake-Ed25519 header construction for indexer calls
        client.rs      HTTP wrappers for /api/inbox, /api/keyrings, /api/workspace, /api/cabinet, /api/events/token
        daemon.rs      Indexer-coordinated maintenance tasks (subset of the core daemon registry)
        types.rs       InboxGrant, KeyringEntry, WorkspaceEntry, etc.
        sse/           SseEvent enum + reconnecting consumer (wasm + native transports)
        tree_keeper/
          mod.rs       TreeKeeper — per-DID in-memory directory tree state. Bootstrapped from loadTree, patched incrementally by document/directory SSE events. `watchDirectory` installs typed snapshot callbacks. Separate Mutex from WorkspaceKeeper.
          tests.rs     Unit tests
        workspace_keeper/
          mod.rs       WorkspaceKeeper — in-memory workspace list state. Bootstrapped by `listWorkspaces`, patched by `keyring:upsert` / `keyring:delete` SSE events. `watchWorkspaces` installs snapshot callbacks. Parallel design to TreeKeeper.
          tests.rs     Unit tests
        inbox_keeper/
          mod.rs       InboxKeeper — in-memory incoming-share list state. Bootstrapped by `listInbox`, patched by `grant:upsert` / `grant:delete` SSE events (indexer fans both to owner and recipient). `watchInbox` installs snapshot callbacks. Parallel design to WorkspaceKeeper; no crypto — entries are already-resolved indexer records.
          tests.rs     Unit tests
      manager/
        mod.rs         FileManager<'a, T, R, S> struct (borrows &mut Opake + &FileContext), create_record passthrough
        types.rs       UploadRequest, UploadResult, DownloadResult, MutationOutcome, FileContext
        upload.rs      upload_at — path-based atomic upload (encrypt + blob + document + directory entry via applyWrites)
        download.rs    download_at — name/path-based cabinet/workspace/cross-PDS download dispatch
        delete.rs      Atomic delete (document + directory entry via applyWrites), delete_recursive (tree-walking)
        move_entry.rs  Atomic move (source removal + target addition via applyWrites)
        directory.rs   ensure_root, create_directory_at (path-based, with duplicate check), delete_directory
        rename.rs      rename_directory (re-encrypt metadata with new name)
        editor.rs      read_metadata (read-only fetch), update_metadata, update_content, fetch_content_key
        sharing.rs     share, revoke_share, list_shares (cabinet-only)
        tree.rs        load_tree, resolve_entry, resolve_document_names, resolve_document_names_in, resolve_document_metadata_in
        admin.rs       WorkspaceAdmin<T, R, S> — add_member, remove_member, leave (keyring membership ops)
        manager_tests.rs  Unit tests
      crypto/
        mod.rs         Type defs (incl. DidMember struct), constants, re-exports
        content.rs     AES-256-GCM: generate_content_key(), encrypt_blob(), decrypt_blob()
        key_wrapping.rs  X25519-HKDF-A256KW: wrap_key(), unwrap_key(), create_group_key()
        keyring_wrapping.rs  Symmetric AES-KW: wrap/unwrap content key under group key
        mnemonic/
          mod.rs       Mnemonic type, parse_mnemonic(), wordlist (BIP-39 embedded)
          generate.rs  generate_mnemonic() — entropy → 24 words
          derive.rs    derive_identity_from_mnemonic() — PBKDF2 → HKDF dual-path
          format.rs    format_mnemonic_grid(), parse_mnemonic_grid() — .txt import/export
      records/
        mod.rs         SCHEMA_VERSION, Versioned trait, check_version(), re-exports
        defs.rs        WrappedKey, EncryptionEnvelope, KeyringRef, EncryptedMetadata, Role, KeyringMember, KeyWrapping, DirectKeyWrapping, KeyringKeyWrapping
        directory.rs   Directory (uses KeyWrapping, not Encryption)
        document.rs    DirectEncryption, KeyringEncryption, Encryption, Document
        public_key.rs  PublicKeyRecord, collection/rkey constants
        grant.rs       Grant
        keyring.rs     KeyHistoryEntry, Keyring (with owner field)
        document_update.rs  DocumentUpdate (actionType: updateContent/updateMetadata/supersede)
        directory_update.rs DirectoryUpdate (actionType: addEntry/removeEntry/moveEntry/createDirectory/deleteDirectory/renameDirectory)
        keyring_update.rs   KeyringUpdate (actionType: addMember/removeMember/updateRole/rename/updateDescription/leave)
        account_config.rs   AccountConfig (singleton; proof-of-life heartbeat)
        invitation.rs       Invitation, InvitationAcceptance
        pair_request.rs     PairRequest + tests
        pair_response.rs    PairResponse + tests
        pending_share.rs    PendingShare (local-only queue of in-flight shares)
      client/
        mod.rs         Re-exports
        transport.rs   Transport trait (HTTP abstraction for WASM compat)
        did.rs         Unauthenticated DID resolution and cross-PDS queries
        list.rs        Generic paginated collection fetcher
        dpop.rs        DPoP keypair (P-256/ES256) + proof JWT generation
        oauth_discovery.rs  OAuth AS discovery + PKCE S256 generation
        oauth_token.rs PAR, authorization code exchange, token refresh (all with DPoP), build_client_id
        oauth_discovery.rs also provides AuthorizationServerMetadata::par_endpoint()
        xrpc/
          mod.rs       XrpcClient struct, Session enum (Legacy/OAuth), dual auth dispatch
          auth.rs      login(), refresh_session() (legacy + OAuth)
          blobs.rs     upload_blob(), get_blob()
          repo.rs      create_record(), put_record(), get_record(), list_records(), delete_record(), apply_writes(). ApplyWriteOp::Create has optional rkey for client-generated TIDs
      directories/
        mod.rs         Re-exports, constants, pub(crate) encrypt_directory_envelope/encrypt_keyring_directory_envelope, workspace root helpers
        create.rs      create_directory()
        delete.rs      pub(crate) delete_directory() — single empty directory
        entries.rs     pub(crate) prepare_add_entry(), prepare_remove_entry() — return ApplyWriteOp for batching
        get_or_create_root.rs  pub(crate) root singleton (rkey "self") + workspace root (rkey "ws-{keyring_rkey}")
        list.rs        list_directories()
        move_entry.rs  move_entry(), check_cycle() — atomic move between directories
        tree.rs        DirectoryTree — in-memory snapshot, decrypt_names_with_group_keys, set_root, resolve_directory, has_child_directory, is_document_uri
        remove.rs      remove() — path-aware deletion (recursive, with parent cleanup)
      documents/
        mod.rs         Re-exports, shared test fixtures
        upload.rs      pub(crate) prepare_upload(), prepare_upload_keyring() — return JSON for applyWrites batching
        download.rs    pub(crate) download() — direct-encrypted documents
        download_grant.rs  download_shared() — cross-PDS via grant URI
        download_keyring.rs  pub(crate) download_keyring_document(), download_with_group_key(), fetch_content_key_with_group_key()
        list.rs        list_documents()
        delete.rs      delete_document()
        resolve.rs     Filename → AT-URI resolution
      metadata/
        mod.rs         Re-exports
        read.rs        Fetch document record + decrypt metadata
        write.rs       Mutate metadata + re-encrypt + putRecord
      keyrings/
        mod.rs         Re-exports, DidMember (replaces MemberKey), resolve_keyring_uri()
        create.rs      create_keyring() → group key + record
        list.rs        list_keyrings()
        add_member.rs  add_member(), AddMemberParams
        remove_member.rs remove_member() — rotate GK, re-wrap to remaining
      sharing/
        mod.rs         Re-exports
        create.rs      create_grant()
        list.rs        list_grants()
        revoke.rs      revoke_grant()
      pairing/
        mod.rs         Re-exports
        request.rs     create_pair_request() — write ephemeral key to PDS, persist privkey via Storage
        respond.rs     respond_to_pair_request() — encrypt + wrap identity (existing device)
        receive.rs     try_complete_pair() — poll, decrypt, save Identity, tear down pair state
        cancel.rs      cancel_pair_request() — wipe pair state on user back-out
        cleanup.rs     cleanup_pair_records() — daemon sweep for expired/orphan records

  opake-wasm/          WASM bridge (wasm-pack, wasm_bindgen)
    src/
      lib.rs           Module declarations, WASM init, pure crypto + tree exports (stateless)
      auth_wasm.rs     OAuth login WASM exports: startOAuthLogin, completeOAuthLogin, loginWithAppPasswordWasm. All token handling in WASM.
      opake_wasm.rs    WasmOpakeHandle (exported to JS as `OpakeContext`). Owns Rc<Mutex<WasmOpake>> shared with WasmFileManagerHandle; short-lived FileManager borrows per JS call. Exports: tokenExpiresAt, proactiveRefresh, checkSession, wipeState, getDid (cached by the SDK at init time into `opake.did`).
      file_manager_wasm.rs  WasmFileManagerHandle (exported to JS as `FileManager`). File operations within a cabinet or workspace context.
      sse_wasm.rs      SSE consumer bindings: startSseConsumer, stopSseConsumer, watchWorkspaces, watchInbox
      pair_wasm.rs     New-device pair bindings: createPairRequest, tryCompletePair, cancelPairRequest. Top-level functions (not on OpakeContext) — they run pre-identity and take JsStorageAdapter + DID directly. No key material crosses the JS boundary.
      js_storage.rs    JsStorage — Storage impl that calls back into a JS-side IndexedDbStorage adapter.
      daemon.rs        WASM bindings for the daemon task registry (daemonTaskDefs) and the default-interval/TTL constants the Service Worker shares with the CLI. Maintenance ops themselves (session refresh, pair cleanup, stale-grant healing, share retry) are OpakeContext methods in opake_wasm.rs.
      wasm_util.rs     make_opake_from_storage, cabinet_context helpers; wasm_err / pub_key_from_slice / to_js / parse_role / build_snapshot utilities; DTOs (DownloadResult, MutationResultDto). WasmOpake = Opake<WasmTransport, OsRng, JsStorage>. Workspace contexts only via opake.workspaceByUri()/resolve — no pre-resolved-key escape hatch.

  opake-derive/        Proc-macro crate
    src/
      lib.rs           #[derive(RedactedDebug)] — generates Debug + Zeroize + Drop for structs with #[redact] fields
                        #[signoff] — attribute macro for session persistence, auto-generates wrapper+inner split. #[signoff] for FileManager (self.opake.signoff()), #[signoff(self)] for Opake (self.signoff())

apps/
  cli/                 CLI binary wrapping opake-core (package: opake-cli)
    src/
      main.rs          Clap app, command dispatch. Workspace command (alias for hidden Keyring)
      config.rs        FileStorage (impl Storage for filesystem), anyhow wrappers
      session.rs       CommandContext, build_opake() (the single CLI Opake construction path — shared by CommandContext::opake and the daemon), chrono_now_micros
      identity.rs      Identity loading, migration (signing keys), permission checks
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
        cat.rs         Alias for `download --stdout`
        ls.rs          Directory-aware listing with --workspace, --long, --tag, optional path
        metadata.rs    View/edit document metadata (rename, tags, description)
        mkdir.rs       create_directory_at — personal or --workspace, path-based
        tree.rs        Unified cabinet/workspace tree with --workspace flag
        rm.rs          delete_recursive via FileManager, tree-walking delete
        move_cmd.rs    FileManager move_entry
        resolve.rs     Identity resolution display
        share.rs       FileManager share() for grant creation
        share_group.rs Share subcommand group (new, revoke, list, inbox)
        revoke.rs      Grant deletion
        shared.rs      List created grants
        inbox.rs       List received grants (via Indexer)
        workspace.rs   Workspace CRUD (create, ls, add-member, leave). remove-member via WorkspaceAdmin. Renamed from keyring.rs
        pair.rs        Device pairing (request, approve)
        accounts.rs    List accounts
        logout.rs      Remove account
        set_default.rs Switch default account
        session_cmd.rs Session token management (inspect, refresh)
        daemon/        Background daemon (session refresh, pair cleanup, grant healing, share retry). Builds Opake per account per task iteration
        config.rs      View/edit account config
        completions.rs Shell completion script generation
        purge.rs       Delete all records from PDS (danger zone)

  web/                 React SPA (Vite + TanStack Start + Tailwind + daisyUI)
    src/
      client.tsx       SPA entry point
      router.tsx       TanStack Router config
      lib/
        atUri.ts             AT-URI parsing helpers
        cn.ts                Tailwind class merge utility
        directoryTree.ts     Tree traversal (parent lookup, ancestor chain, path suffix)
        docs-registry.ts     Documentation section metadata
        download.ts          Browser file-download helpers
        encoding.ts          Base64/hex/fingerprint utilities
        fileContext.ts       Discriminated cabinet-vs-workspace file context
        format.ts            Size/date/text formatters
        og-meta.ts           Open Graph meta tag helpers
        pairing.ts           Device pairing (thin wrappers over @opake/sdk)
        pdsTypes.ts          Re-exports of @opake/sdk DTO types
        persistent-storage.ts navigator.storage persistence request
        profileResolution.ts Bluesky profile lookup
        resizeImage.ts       Image resize for uploads
        seedPhraseParser.ts  Seed phrase text extraction (numbered grids, plain lists)
        sharing.ts           Share dialog helpers
        workspaceSchemas.ts  Workspace form validation
      stores/
        auth.ts          Auth + identity state machine (Zustand + @opake/sdk)
        app.ts           App-wide loading tracker
        tasks.ts         Daemon task status display
        toast.ts         Toast notifications
      routes/            TanStack Router file-based routing
        __root.tsx       Root layout (HTML shell, error boundary)
        _public.tsx      Public layout (nav, footer — SSR)
        _public/         Public routes (landing, docs, FAQ)
        cabinet/         Cabinet + workspace routes (lazy-loaded, auth-guarded)
        devices/         Device + identity routes (lazy-loaded)
      components/
        cabinet/         File browser, sidebar, editor, workspace UI
          FileView.tsx         Unified cabinet + workspace file browser
          EditorView.tsx       Shared markdown editor shell (edit / new modes)
          MarkdownEditor.tsx   Tiptap-based editor surface
          MarkdownPreview.tsx  Rendered markdown with mermaid support
          FilePreview.tsx      Side-panel content preview
          DirectoryReadme.tsx  README auto-render
          PanelContent.tsx     File grid/list view
          PanelShell.tsx       Panel container
          Sidebar.tsx          Navigation sidebar
          TopBar.tsx           Header with account switcher
          Breadcrumbs.tsx      Path breadcrumbs
          (+ dialogs and smaller widgets: AddMember, CreateWorkspace, Delete, Move,
             NewFolder, Rename, Share, ShareManagement, Revoke, Invite, Metadata,
             WorkspaceMembers, WorkspaceSettings, ImageInsert, etc.)
        devices/         Identity setup, seed phrase, pairing, conflict resolution
        content/         MDX rendering, landing page sections

    WASM lives in `packages/opake-sdk/wasm/` — built by `just wasm` and imported
    via `@opake/sdk`. No web-local worker layer: all file/identity operations go
    through `@opake/sdk` and `@opake/react` hooks on the main thread.

apps/indexer/         Elixir/Phoenix indexer + REST API + SSE broadcaster
  lib/
    opake_indexer/
      application.ex       OTP supervision tree (Repo, KeyCache, Endpoint, Jetstream consumer, SSE ETS tables)
      firehose.ex          Event dispatch, cursor saving, connection state
      firehose/            Firehose runtime state + heartbeat
      release.ex           Release tasks (create_db, migrate, rollback, status)
      repo.ex              Ecto Repo
      backfill.ex          Historical ingestion of a DID's app.opake.* records
      tombstone_cleanup.ex Periodic cleanup of tombstoned records
      auth/
        plug.ex            Opake-Ed25519 header verification (Plug)
        key_cache.ex       GenServer + ETS, 5-min TTL per DID
        key_fetcher.ex     DID → PDS → publicKey → signingKey resolution
        base64.ex          Flexible base64 decode (padded/unpadded)
      jetstream/
        consumer.ex        WebSockex client with exponential backoff
        event.ex           Jetstream JSON → tagged tuples
        compression.ex     Per-consumer zstd streaming context
      sse/
        broadcaster.ex     PubSub fan-out for every indexed event
        topics.ex          Topic builders (personal, workspace)
        token_store.ex     Single-use opaque tokens for /api/events
        connection_tracker.ex  ETS-backed per-DID SSE connection cap
      queries/
        cursor_queries.ex  Singleton cursor upsert/load
        grant_queries.ex   Grant CRUD + inbox pagination
        keyring_queries.ex Keyring + keyring_member CRUD and membership pagination
        document_queries.ex  Keyring-encrypted document index queries
        document_update_queries.ex  Document update proposal queries
        directory_queries.ex  Directory + directory_update queries, cabinet/workspace snapshot + sync
        pagination.ex      Shared cursor-based pagination helpers
      schemas/
        cursor.ex          Singleton cursor (id=1)
        grant.ex           Grant (uri PK)
        keyring.ex         Keyring (uri PK, owner DID, metadata)
        keyring_member.ex  Keyring member (composite PK: keyring_uri + member_did)
        keyring_update.ex  Keyring update proposal
        document.ex        Keyring-encrypted document
        document_update.ex Document update proposal
        directory.ex       Workspace directory
        directory_update.ex Directory update proposal
    opake_indexer_web/
      router.ex             /api/health (public); everything else auth'd: inbox, keyrings, workspace/{documents,updates,directory-updates,snapshot,sync}, cabinet/{snapshot,sync}, events + events/token
      endpoint.ex           Bandit HTTP, long SSE read timeouts
      plugs/rate_limit.ex   Hammer ETS rate limiting per IP (skipped for /api/events)
      controllers/
        health_controller.ex     Indexer status + cursor lag
        inbox_controller.ex      Grants by recipient DID
        keyrings_controller.ex   Keyrings by member DID
        workspace_controller.ex  Workspace documents, update proposals, snapshot + sync
        cabinet_controller.ex    Cabinet snapshot + sync (personal tree)
        events_controller.ex     POST /events/token + GET /events (chunked SSE)
        pagination_helpers.ex    Shared param parsing (did, limit, cursor, since)
        tree_helpers.ex          Snapshot/sync response shaping

packages/
  opake-sdk/             @opake/sdk — TypeScript SDK wrapping the WASM bindings
    src/
      index.ts           Package entry point, re-exports
      opake.ts           Opake client (auth, identity, workspaces, SSE, daemon ops)
      file-manager.ts    FileManager (upload, download, tree, metadata, directory CRUD)
      auth.ts            OAuth/DPoP two-step login + app password flows
      pairing.ts         Device pairing — new-device statics (createPairRequest / awaitPairCompletion / cancelPairRequest, take Storage+DID) + existing-device helpers (listPairRequests / approvePairRequest / cleanupExpiredPairRequests, take an Opake context)
      schemas.ts         Zod schemas for WASM return values (runtime validation at the boundary)
      storage.ts         Storage interface (mirrors the opake-core trait)
      storage-adapter.ts JS-side adapter that the Rust JsStorage binds against
      storage/
        indexeddb.ts     Dexie-based IndexedDbStorage
        memory.ts        In-memory Storage for tests
      wasm.ts            WASM init + bridge helpers
      finalizer.ts       FinalizationRegistry wrapper for WASM handle disposal
      types.ts           Domain type exports (DirectoryTreeSnapshot, WorkspaceEntry, etc.)
      errors.ts          Typed error hierarchy
    wasm/                wasm-pack output (imported by the SDK)

  opake-daemon/          @opake/daemon — Background task scheduler (browser-main-thread)
    src/
      index.ts           Package entry point
      scheduler.ts       startDaemon()  — interval-driven task loop
      tasks.ts           Task definitions (session refresh, pair cleanup, grant healing, share retry)
      types.ts           Task type definitions

  opake-react/           @opake/react — React 19 hooks over @opake/sdk
    src/
      index.ts           Package entry point
      provider.tsx       OpakeProvider (FileManagerCache + OptimisticOverlay + SSE lifecycle)
      file-manager-cache.ts   Refcounted per-context FileManager cache
      optimistic-overlay.ts   Per-scope snapshot-transforming patch store
      keys.ts            React Query key factories
      hooks/
        bootstrap-once.ts             Deduped (Opake, label) bootstrap guard
        use-directory.ts              Subscription tree hook (SSE-driven)
        use-directory-metadata.ts     Per-directory document-metadata query
        use-directory-mutations.ts    Create/rename/delete directory mutations
        use-file-manager.ts           Low-level FileManager acquisition
        use-tree.ts                   Deprecated react-query tree reader
        use-tree-mutation.ts          Shared mutation helper (optimistic overlay + invalidation)
        use-upload.ts / use-delete.ts / use-move.ts / use-download.ts
        use-workspaces.ts / use-inbox.ts / use-shares.ts / use-pending-shares.ts
        use-share-mutations.ts / use-create-workspace.ts
        use-daemon.ts                 Background task status
        use-start-sse-consumer.ts     Standalone SSE start primitive (provider does this already)

tests/                   Cross-package integration tests (CLI-driven Rust tests)
```

The boundary is strict: `opake-core` never touches the filesystem, stdin, or any platform-specific API. All I/O happens through the `Storage` trait — `FileStorage` (CLI, filesystem) and `IndexedDbStorage` (web, IndexedDB) implement the same contract with platform-specific backends. This keeps `opake-core` compilable to WASM. The `@opake/sdk` package wraps the WASM bindings in a TypeScript API; the web frontend consumes Opake through the SDK, not raw WASM imports.

The `Opake<T, R, S>` struct is the root context for all operations. A constructed `Opake` always has an Identity — `for_account` returns `Error::IdentityMissing` for authenticated accounts without one, and callers route to recovery or pairing to bootstrap. All CLI commands route through Opake except `pair request` and `recover` (neither has an Identity yet — both use the storage-backed free functions in `opake_core::pairing` and the raw XRPC client). Construct one with an authenticated client + identity + storage, then call `.file_context(workspace_name?)` to resolve the target, `.file_manager(&context)` for file operations, or `.workspace_admin()` for membership management (add/remove member, leave). Opake itself provides workspace CRUD, sharing (grants, pending shares), identity/account management, the existing-device side of pairing (list/approve), and maintenance methods. Session persistence is automatic via `#[signoff]` / `#[signoff(self)]` on every public mutation. Platform differences (transport, RNG, clock, storage) are injected via type parameters and function pointers — no conditional compilation in the domain layer.
