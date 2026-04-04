# Opake — Monorepo Structure

```
crates/
  opake-core/          Platform-agnostic library (compiles to WASM)
    src/
      opake.rs         Opake<T, R, S> root context — owns storage, factory for FileManager/WorkspaceAdmin. All CLI commands (except pair request) route through here. Methods for workspace mgmt, sharing, identity, pairing, maintenance, low-level records. signoff() auto-persists session
      cabinet.rs       Cabinet domain type (personal file space). ZeroizeOnDrop
      workspace.rs     Workspace domain type (shared file space, wraps keyring data). ZeroizeOnDrop
      tid.rs           TID (Timestamp ID) generator for client-side AT Protocol rkeys
      atproto.rs       AT-URI parsing, shared AT Protocol primitives
      account_config.rs  Fetch/publish singleton account config from PDS
      resolve.rs       Handle/DID → PDS → public key resolution pipeline
      storage.rs       Config, Identity types + Storage trait (cross-platform contract)
      paths.rs         Data directory resolution (env, XDG, fallback)
      daemon.rs        Background task registry (shared definitions for CLI + web). Daemon builds Opake per account per task iteration, auto-persists via signoff
      error.rs         Typed error hierarchy (thiserror)
      test_utils.rs    MockTransport + response queue (behind test-utils feature)
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
        keyring_leave.rs    KeyringLeave, KEYRING_LEAVE_COLLECTION
      client/
        mod.rs         Re-exports
        transport.rs   Transport trait (HTTP abstraction for WASM compat)
        did.rs         Unauthenticated DID resolution and cross-PDS queries
        list.rs        Generic paginated collection fetcher
        dpop.rs        DPoP keypair (P-256/ES256) + proof JWT generation
        oauth_discovery.rs  OAuth AS discovery + PKCE S256 generation
        oauth_token.rs PAR, authorization code exchange, token refresh (all with DPoP)
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
        request.rs     create_pair_request() — write ephemeral key to PDS
        respond.rs     respond_to_pair_request() — encrypt + wrap identity
        receive.rs     receive_pair_response() — decrypt + verify identity
        cleanup.rs     cleanup_pair_records() — delete request + response

  opake-wasm/          WASM bridge (wasm-pack, wasm_bindgen)
    src/
      lib.rs           Module declarations, WASM init, pure crypto + tree exports (stateless)
      opake_wasm.rs    OpakeContext + WasmFileManagerHandle (owns Opake+FileContext, temporary FileManager borrows per JS call)
      daemon.rs        Service Worker maintenance task exports (session refresh, pair cleanup)
      wasm_util.rs     make_client, make_opake, make_cabinet, make_workspace helpers. WasmOpake = Opake<WasmTransport, OsRng, NoopStorage>

  opake-derive/        Proc-macro crate
    src/
      lib.rs           #[derive(RedactedDebug)] — generates Debug + Zeroize + Drop for structs with #[redact] fields
                        #[signoff] — attribute macro for session persistence, auto-generates wrapper+inner split. #[signoff] for FileManager (self.opake.signoff()), #[signoff(self)] for Opake (self.signoff())

apps/
  cli/                 CLI binary wrapping opake-core (package: opake-cli)
    src/
      main.rs          Clap app, command dispatch. Workspace command (alias for hidden Keyring)
      config.rs        FileStorage (impl Storage for filesystem), anyhow wrappers
      session.rs       CommandContext, opake() factory (passes FileStorage to Opake), chrono_now/chrono_now_micros
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
        inbox.rs       List received grants (via AppView)
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
      lib/
        encoding.ts      Base64/hex/fingerprint utilities
        atUri.ts         AT-URI parsing helpers
        pairing.ts       Device pairing (thin wrappers over @opake/sdk)
        seedPhraseParser.ts  Seed phrase text extraction (numbered grids, plain lists)
        cn.ts            Tailwind class merge utility
        docs-registry.ts Documentation section metadata
        og-meta.ts       Open Graph meta tag helpers
      stores/
        auth.ts          Auth + identity state machine (Zustand + @opake/sdk)
        app.ts           App-wide loading tracker
        toast.ts         Toast notifications
      routes/            TanStack Router file-based routing
        __root.tsx       Root layout (HTML shell, error boundary)
        _public.tsx      Public layout (nav, footer — SSR)
        _public/         Public routes (landing, docs, FAQ)
        cabinet/         Cabinet routes (lazy-loaded, auth-guarded)
        devices/         Device + identity routes (lazy-loaded)
      components/
        cabinet/         File browser, sidebar, editor, workspace UI
        devices/         Identity setup, seed phrase, pairing, conflict resolution
        content/         MDX rendering, landing page sections
        PanelContent.tsx     File grid/list view
        PanelShell.tsx       Panel container
        Sidebar.tsx          Navigation sidebar
        TopBar.tsx           Header with account switcher
        FileGridCard.tsx     Grid card with file icon + metadata
        FileListRow.tsx      List row variant
        FilePreview.tsx      File content preview
        MarkdownEditor.tsx   In-browser markdown editing
        SearchResults.tsx    Search results panel
        types.ts             Discriminated union types for cabinet state
        (+ dialogs: AddMember, CreateWorkspace, Delete, Move, NewFolder, Rename, Share, WorkspaceMembers, WorkspaceSettings, etc.)
      wasm/opake-wasm/   WASM build of opake-core (via wasm-pack)
      workers/
        opake.worker.ts  Single worker composing all API modules (Comlink)
        daemon.ts        Background daemon (session refresh, cleanup tasks)
        context.ts       Worker context management
        api/
          cabinet.ts     Cabinet file operations
          identity.ts    Keypairs, seed phrases, DPoP, DID resolution
          workspace.ts   Workspace operations

  appview/             Elixir/Phoenix indexer + REST API (replaces Rust appview)
    lib/
      opake_appview/
        application.ex       OTP supervision tree (Repo, KeyCache, Endpoint, Consumer)
        indexer.ex            Event dispatch, cursor saving, connection state (ETS)
        release.ex            Release tasks (create_db, migrate, rollback, status)
        repo.ex               Ecto Repo
        auth/
          plug.ex             Opake-Ed25519 header verification (Plug)
          key_cache.ex        GenServer + ETS, 5-min TTL per DID
          key_fetcher.ex      DID → PDS → publicKey → signingKey resolution
          base64.ex           Flexible base64 decode (padded/unpadded)
        jetstream/
          consumer.ex         WebSockex client with exponential backoff
          event.ex            Jetstream JSON → tagged tuples
        queries/
          cursor_queries.ex   Singleton cursor upsert/load
          grant_queries.ex    Grant CRUD + inbox pagination
          keyring_queries.ex  Keyring member CRUD + membership pagination
          workspace_queries.ex  Workspace document membership queries
          document_update_queries.ex  Document update index queries
          pagination.ex       Shared cursor-based pagination helpers
        schemas/
          cursor.ex           Singleton cursor (id=1)
          grant.ex            Grant (uri PK)
          keyring_member.ex   Keyring member (composite PK)
          workspace_document.ex  Workspace document schema
          document_update.ex  Document update schema
      opake_appview_web/
        router.ex             /api/health (public), /api/inbox + /api/keyrings + /api/workspace + /api/workspace/updates (auth'd)
        endpoint.ex           Bandit HTTP, API-only (no sessions/static)
        plugs/rate_limit.ex   Hammer ETS rate limiting per IP
        controllers/
          health_controller.ex     Indexer status + cursor lag
          inbox_controller.ex      Grants by recipient DID
          keyrings_controller.ex   Keyrings by member DID
          workspace_controller.ex  Workspace documents + pending updates
          pagination_helpers.ex    Shared param parsing (did, limit, cursor)

packages/
  opake-sdk/             @opake/sdk — TypeScript SDK wrapping WASM bindings
    src/
      index.ts           Package entry point, re-exports
      opake.ts           Opake client (auth, identity, workspaces, daemon ops)
      file-manager.ts    FileManager (upload, download, tree, metadata)
      auth.ts            OAuth/DPoP two-step login + app password flows
      pairing.ts         Device pairing (create/approve/receive/cleanup)
      storage.ts         Storage interface (mirrors opake-core trait)
      wasm.ts            WASM initialization and bridge
      types.ts           Domain type definitions (results, pairing, workspaces)
      errors.ts          Typed error hierarchy
      storage/           Storage implementations
    wasm/                WASM build output (wasm-pack → here)

  opake-daemon/          @opake/daemon — Background task scheduler
    src/
      index.ts           Package entry point
      scheduler.ts       Task scheduling loop
      tasks.ts           Task definitions (session refresh, pair cleanup, grant healing)
      types.ts           Task type definitions

  opake-react/           @opake/react — React bindings
    src/
      index.ts           Package entry point
      provider.tsx       OpakeProvider context
      keys.ts            Query key management
      hooks/             React hooks for Opake operations

tests/                   E2E and integration tests
  tests/
    cli/                 CLI integration tests
    web/                 Web E2E tests (Playwright)
```

The boundary is strict: `opake-core` never touches the filesystem, stdin, or any platform-specific API. All I/O happens through the `Storage` trait — `FileStorage` (CLI, filesystem) and `IndexedDbStorage` (web, IndexedDB) implement the same contract with platform-specific backends. This keeps `opake-core` compilable to WASM. The `@opake/sdk` package wraps the WASM bindings in a TypeScript API; the web frontend consumes Opake through the SDK, not raw WASM imports.

The `Opake<T, R, S>` struct is the root context for all operations. All CLI commands route through Opake except `pair request` (new device has no identity yet — uses raw pairing functions). Construct one with an authenticated client + identity + storage, then call `.file_context(workspace_name?)` to resolve the target, `.file_manager(&context)` for file operations, or `.workspace_admin()` for membership management (add/remove member, leave). Opake itself provides workspace CRUD, sharing (grants, pending shares), identity/account management, pairing, and maintenance methods. Session persistence is automatic via `#[signoff]` / `#[signoff(self)]` on every public mutation. Platform differences (transport, RNG, clock, storage) are injected via type parameters and function pointers — no conditional compilation in the domain layer.
