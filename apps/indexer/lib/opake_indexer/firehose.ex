defmodule OpakeIndexer.Firehose do
  @moduledoc """
  Dispatches parsed Jetstream events to query modules and maintains the
  shared `OpakeIndexer.Firehose.State`.

  ## Per-event flow

      raw json
        ↓
      Event.parse/1          (single Jason.decode + tagged tuple)
        ↓
      bump counters + last_event_at
        ↓
      dispatch (if not :ignore)
        ↓
      maybe_save_cursor      (time-throttled, see @cursor_save_interval_ms)
        ↓
      :telemetry.execute     (see :opake_indexer events below)

  ## Cursor save policy

  Cursor saves are time-throttled, not count-throttled. We persist at most
  once every `@cursor_save_interval_ms` regardless of event volume. The
  previous count-based policy hammered Postgres at firehose rates because
  the constant was set to 1 (see git history).

  ## Telemetry events

    * `[:opake_indexer, :indexer, :event]`
      Measurement: `%{count: 1}`
      Metadata: `%{collection: "app.opake.grant", action: :upsert | :delete | :ignore, status: :ok | :error | :ignored}`

    * `[:opake_indexer, :indexer, :cursor_saved]`
      Measurement: `%{time_us: integer}`
      Metadata: `%{}`

  Cursor saves are time-throttled in tests (interval forced to 0) so each
  call to `process_message/2` writes the cursor — keeps the existing
  pipeline tests behaviorally identical.
  """

  require Logger

  alias OpakeIndexer.Jetstream.Event
  alias OpakeIndexer.Firehose.State

  alias OpakeIndexer.Queries.{
    CursorQueries,
    DirectoryQueries,
    DocumentQueries,
    DocumentUpdateQueries,
    GrantQueries,
    KeyringQueries
  }

  alias OpakeIndexer.SSE.Broadcaster

  @telemetry_prefix [:opake_indexer, :indexer]

  # -- State init --

  @doc """
  Creates the `:indexer_state` ETS table. Called once from
  `OpakeIndexer.Application.start/2`.
  """
  defdelegate init_state, to: State, as: :init

  defdelegate set_connected(connected), to: State

  defdelegate connected?, to: State

  # -- Configuration --

  defp cursor_save_interval_ms do
    Application.get_env(:opake_indexer, :cursor_save_interval_ms, 5_000)
  end

  # -- Public entry point --

  @doc """
  Process a raw Jetstream JSON frame. Returns the updated event_count
  (preserved for backwards compat with the existing `WebSockex` consumer
  loop and the pipeline tests).
  """
  @spec process_message(binary(), non_neg_integer()) :: non_neg_integer()
  def process_message(json, event_count) do
    now = DateTime.utc_now()

    {time_us, collection, payload} = Event.parse(json)

    State.bump_total()
    State.mark_event_received()
    if collection, do: State.bump_collection(collection)

    case payload do
      :ignore ->
        State.bump_ignored()
        emit_event_telemetry(collection, :ignore, :ignored)

      _ ->
        State.bump_indexed()
        dispatch(payload, time_us, now)
    end

    maybe_save_cursor(time_us)
    event_count + 1
  end

  # -- Dispatch --
  #
  # Every clause here is, by construction, processing an opake event —
  # the parser already routed everything else into the :ignore branch
  # upstream. So per-event info logging is safe: there's no flooding
  # risk from bsky traffic. Opake events are rare and meaningful, and
  # their log lines are exactly the operational signal you want to see
  # ("the indexer just indexed a real user action").
  #
  # Errors are logged but not raised — one bad event cannot stall the
  # whole indexer.

  defp dispatch({:upsert_grant, attrs}, _time_us, now) do
    Logger.info(
      "[Indexer] grant upsert: #{attrs.uri} (owner=#{attrs.owner_did}, recipient=#{attrs.recipient_did})"
    )

    result =
      GrantQueries.upsert_grant(%{
        uri: attrs.uri,
        owner_did: attrs.owner_did,
        recipient_did: attrs.recipient_did,
        document_uri: attrs.document_uri,
        created_at: attrs.created_at,
        indexed_at: now
      })

    log_query_error(result, "grant upsert", attrs.uri)
    emit_event_telemetry("app.opake.grant", :upsert, status_of(result))
    Broadcaster.broadcast_grant(attrs, :upsert)
  end

  defp dispatch({:delete_grant, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] grant delete: #{uri}")
    # Fetch parties before deleting — the firehose delete payload carries only
    # the URI. We need owner_did + recipient_did to fan out SSE deletes to both
    # personal topics. If the row is already gone (idempotent replay), parties
    # is nil and the broadcaster falls back to owner-only via the uri attrs.
    parties = GrantQueries.grant_parties(uri)
    GrantQueries.delete_grant(uri)
    emit_event_telemetry("app.opake.grant", :delete, :ok)
    attrs = case parties do
      {owner_did, recipient_did} -> %{uri: uri, owner_did: owner_did, recipient_did: recipient_did}
      nil -> %{uri: uri}
    end
    Broadcaster.broadcast_grant(attrs, :delete)
  end

  defp dispatch({:upsert_keyring, attrs}, _time_us, _now) do
    Logger.info(
      "[Indexer] keyring upsert: #{attrs.uri} (owner=#{attrs.owner_did}, members=#{length(attrs.member_entries)})"
    )

    members_result =
      KeyringQueries.upsert_keyring(attrs.uri, attrs.owner_did, attrs.member_entries)

    log_query_error(members_result, "keyring members upsert", attrs.uri)

    record_result = KeyringQueries.upsert_keyring_record(attrs)
    log_query_error(record_result, "keyring record upsert", attrs.uri)

    emit_event_telemetry(
      "app.opake.keyring",
      :upsert,
      worst_status([members_result, record_result])
    )

    Broadcaster.broadcast_keyring(attrs, :upsert)
  end

  defp dispatch({:delete_keyring, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] keyring delete: #{uri}")
    KeyringQueries.delete_keyring(uri)
    emit_event_telemetry("app.opake.keyring", :delete, :ok)
    Broadcaster.broadcast_keyring(%{uri: uri, owner_did: nil}, :delete)
  end

  defp dispatch({:upsert_directory, attrs}, _time_us, now) do
    Logger.info("[Indexer] directory upsert: #{attrs.directory_uri}")

    result =
      DirectoryQueries.upsert_directory(%{
        directory_uri: attrs.directory_uri,
        keyring_uri: attrs.keyring_uri,
        owner_did: attrs.owner_did,
        entries: attrs.entries,
        encrypted_metadata: attrs.encrypted_metadata,
        key_wrapping: attrs.key_wrapping,
        modified_at: attrs[:modified_at],
        deleted_at: nil,
        indexed_at: now
      })

    log_query_error(result, "directory upsert", attrs.directory_uri)
    emit_event_telemetry("app.opake.directory", :upsert, status_of(result))
    Broadcaster.broadcast_directory(attrs, :upsert)
  end

  defp dispatch({:delete_directory, %{directory_uri: directory_uri}}, _time_us, now) do
    Logger.info("[Indexer] directory delete (soft): #{directory_uri}")
    DirectoryQueries.soft_delete_directory(directory_uri, now)
    emit_event_telemetry("app.opake.directory", :delete, :ok)
    Broadcaster.broadcast_directory(%{directory_uri: directory_uri, owner_did: nil}, :delete)
  end

  defp dispatch({:upsert_document, attrs}, _time_us, now) do
    Logger.info("[Indexer] document upsert: #{attrs.document_uri}")

    result =
      DocumentQueries.upsert_document(%{
        document_uri: attrs.document_uri,
        keyring_uri: attrs.keyring_uri,
        owner_did: attrs.owner_did,
        rotation: attrs.rotation,
        encrypted_metadata: attrs.encrypted_metadata,
        encryption: attrs.encryption,
        blob_ref: attrs.blob_ref,
        modified_at: attrs[:modified_at],
        deleted_at: nil,
        indexed_at: now
      })

    log_query_error(result, "document upsert", attrs.document_uri)
    emit_event_telemetry("app.opake.document", :upsert, status_of(result))
    Broadcaster.broadcast_document(attrs, :upsert)
  end

  defp dispatch({:delete_document, %{document_uri: document_uri}}, _time_us, now) do
    Logger.info("[Indexer] document delete (soft): #{document_uri}")
    DocumentQueries.soft_delete_document(document_uri, now)
    emit_event_telemetry("app.opake.document", :delete, :ok)
    Broadcaster.broadcast_document(%{document_uri: document_uri, owner_did: nil}, :delete)
  end

  defp dispatch({:upsert_document_update, attrs}, _time_us, now) do
    Logger.info("[Indexer] document update: #{attrs.uri} -> #{attrs.document_uri}")

    result =
      DocumentUpdateQueries.upsert_document_update(%{
        uri: attrs.uri,
        document_uri: attrs.document_uri,
        author_did: attrs.author_did,
        indexed_at: now
      })

    log_query_error(result, "document update upsert", attrs.uri)
    emit_event_telemetry("app.opake.documentUpdate", :upsert, status_of(result))

    # Enrich the broadcast with the workspace keyring URI so the
    # broadcaster can route to the workspace topic (where the document
    # owner is subscribed). The `app.opake.documentUpdate` lexicon has
    # no `keyring` field, so we join through the documents table here.
    # If the document hasn't been indexed yet (race), fall through to
    # the personal-topic broadcast path; the owner's next
    # `sync_workspace_by_uri` call will still pick up the proposal.
    enriched =
      case DocumentQueries.keyring_uri_for_document(attrs.document_uri) do
        {:ok, keyring_uri} -> Map.put(attrs, :keyring_uri, keyring_uri)
        :not_found -> attrs
      end

    Broadcaster.broadcast_document_update(enriched, :upsert)
  end

  defp dispatch({:delete_document_update, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] document update delete: #{uri}")
    DocumentUpdateQueries.delete_document_update(uri)
    emit_event_telemetry("app.opake.documentUpdate", :delete, :ok)
    Broadcaster.broadcast_document_update(%{uri: uri}, :delete)
  end

  defp dispatch({:upsert_directory_update, attrs}, _time_us, now) do
    Logger.info(
      "[Indexer] directory update: #{attrs.uri} -> #{attrs.keyring_uri} (#{attrs.action_type})"
    )

    result =
      DirectoryQueries.upsert_directory_update(%{
        uri: attrs.uri,
        keyring_uri: attrs.keyring_uri,
        author_did: attrs.author_did,
        action_type: attrs.action_type,
        directory_uri: attrs.directory_uri,
        entry_uri: attrs.entry_uri,
        encrypted_metadata: attrs[:encrypted_metadata],
        source_directory_uri: attrs[:source_directory_uri],
        target_directory_uri: attrs[:target_directory_uri],
        parent_directory_uri: attrs[:parent_directory_uri],
        indexed_at: now
      })

    log_query_error(result, "directory update upsert", attrs.uri)
    emit_event_telemetry("app.opake.directoryUpdate", :upsert, status_of(result))
    Broadcaster.broadcast_directory_update(attrs, :upsert)
  end

  defp dispatch({:delete_directory_update, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] directory update delete: #{uri}")
    DirectoryQueries.delete_directory_update(uri)
    emit_event_telemetry("app.opake.directoryUpdate", :delete, :ok)
    Broadcaster.broadcast_directory_update(%{uri: uri}, :delete)
  end

  defp dispatch({:upsert_keyring_update, attrs}, _time_us, now) do
    Logger.info(
      "[Indexer] keyring update: #{attrs.uri} -> #{attrs.keyring_uri} (#{attrs.action_type})"
    )

    result =
      KeyringQueries.upsert_keyring_update(%{
        uri: attrs.uri,
        keyring_uri: attrs.keyring_uri,
        author_did: attrs.author_did,
        action_type: attrs.action_type,
        member_did: attrs[:member_did],
        member_public_key: attrs[:member_public_key],
        role: attrs[:role],
        encrypted_metadata: attrs[:encrypted_metadata],
        indexed_at: now
      })

    log_query_error(result, "keyring update upsert", attrs.uri)

    # Immediate visibility: "leave" removes the member from the Indexer's index
    if attrs.action_type == "leave" do
      Logger.info("[Indexer] keyring leave: #{attrs.author_did} left #{attrs.keyring_uri}")
      KeyringQueries.remove_member(attrs.keyring_uri, attrs.author_did)
    end

    emit_event_telemetry("app.opake.keyringUpdate", :upsert, status_of(result))
    Broadcaster.broadcast_keyring_update(attrs, :upsert)
  end

  defp dispatch({:delete_keyring_update, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] keyring update delete: #{uri}")
    KeyringQueries.delete_keyring_update(uri)
    emit_event_telemetry("app.opake.keyringUpdate", :delete, :ok)
    Broadcaster.broadcast_keyring_update(%{uri: uri}, :delete)
  end

  # Heartbeat-only path: log the account config write but don't persist.
  # Web clients write `app.opake.accountConfig` periodically as a proof-of-life
  # signal — surfacing it here lets us see the indexer is processing real PDS
  # writes from active sessions even when no file/workspace activity exists.
  defp dispatch({:account_config_seen, %{did: did, op: op}}, _time_us, _now) do
    Logger.info("[Indexer] accountConfig #{op}: #{did}")

    action =
      case op do
        "create" -> :upsert
        "update" -> :upsert
        "delete" -> :delete
      end

    emit_event_telemetry("app.opake.accountConfig", action, :ok)
  end

  # -- Cursor save (time-throttled + monotonic) --
  #
  # Monotonicity matters: in `:full` mode we receive interleaved commits
  # from thousands of PDSes. Jetstream orders events by `time_us` *globally*
  # but retries, small server-side reorderings, and cross-PDS clock skew
  # can still present a slightly older event to us right after a newer one.
  # If we blindly save whatever time_us was latest in the throttle window,
  # the persisted cursor can jump backwards — and on reconnect we'd replay
  # everything we already processed. Save only when the new time_us is
  # strictly greater than the last value we persisted.

  defp maybe_save_cursor(nil), do: :ok

  defp maybe_save_cursor(time_us) when is_integer(time_us) do
    interval = cursor_save_interval_ms()
    age = State.cursor_saved_age_ms()
    throttle_ok? = interval == 0 or age == nil or age >= interval

    if throttle_ok? and monotonic?(time_us) do
      CursorQueries.save_cursor(time_us)
      State.record_cursor_save(time_us)
      Logger.debug(fn -> "Saved cursor at #{time_us}" end)

      :telemetry.execute(
        @telemetry_prefix ++ [:cursor_saved],
        %{time_us: time_us},
        %{}
      )
    end

    :ok
  end

  defp monotonic?(time_us) do
    case State.last_cursor_time_us() do
      nil -> true
      previous when is_integer(previous) -> time_us > previous
    end
  end

  # -- Helpers --

  defp log_query_error({:ok, _}, _label, _uri), do: :ok

  defp log_query_error({:error, reason}, label, uri) do
    Logger.warning("Failed #{label} for #{uri}: #{inspect(reason)}")
  end

  defp log_query_error(_, _, _), do: :ok

  defp status_of({:ok, _}), do: :ok
  defp status_of({:error, _}), do: :error
  defp status_of(_), do: :ok

  defp worst_status(results) do
    if Enum.any?(results, &match?({:error, _}, &1)), do: :error, else: :ok
  end

  defp emit_event_telemetry(collection, action, status) do
    :telemetry.execute(
      @telemetry_prefix ++ [:event],
      %{count: 1},
      %{collection: collection, action: action, status: status}
    )
  end
end
