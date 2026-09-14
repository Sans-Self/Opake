defmodule OpakeIndexer.Firehose do
  @moduledoc """
  Dispatches parsed Jetstream events to the records / chain_heads tables
  and broadcasts envelopes onto SSE topics.

  ## Per-event flow

      raw json
        ↓
      Event.parse/1          (single Jason.decode + tagged tuple)
        ↓
      bump counters
        ↓
      dispatch (if not :ignore)
        ↓
      maybe_save_cursor      (time-throttled, monotonic)
        ↓
      :telemetry.execute

  ## Uniform record path

  Every indexed `at.opake.*` record follows the same pipeline:

    1. Authority check (for chain-bearing supersedes — keyring + root
       directory chains).
    2. Insert/update the `records` row with the verbatim record_jsonb
       and the indexer-managed metadata (`indexed_at`).
    3. Update `chain_heads` if this record participates in a tracked
       chain (keyring genesis/supersede, workspace-root genesis/supersede).
       Same DB transaction as the record insert.
    4. Broadcast the envelope onto the appropriate SSE topics.

  Chain dispatch decides one of:

    * **Genesis** — no `supersedes` field. Create the chain head row.
    * **Advance** — `supersedes` matches the current head URI. Compare-
      and-set the head.
    * **Fork** — `supersedes` points at a non-head URI. Persist the
      record but don't advance; broadcast a `chain:forked` event.
    * **Orphan** — `supersedes` points at a record we haven't seen.
      Persist the record; no chain action. Heals when predecessor arrives.

  Documents and grants don't drive chains. They get an upsert into
  `records` and a broadcast; that's it.
  """

  require Logger

  alias OpakeIndexer.Authority
  alias OpakeIndexer.Jetstream.Event
  alias OpakeIndexer.Firehose.ConsumeLag
  alias OpakeIndexer.Firehose.State
  alias OpakeIndexer.Lexicon.Validator

  alias OpakeIndexer.Queries.{
    ChainHeadQueries,
    CursorQueries,
    RecordQueries
  }

  alias OpakeIndexer.SSE.Broadcaster

  @telemetry_prefix [:opake_indexer, :indexer]

  @directory_collection "at.opake.directory"
  @document_collection "at.opake.document"
  @keyring_collection "at.opake.keyring"
  @grant_collection "at.opake.grant"
  @account_config_collection "at.opake.accountConfig"

  # -- State init --

  defdelegate init_state, to: State, as: :init
  defdelegate set_connected(connected), to: State
  defdelegate connected?, to: State

  # -- Configuration --

  defp cursor_save_interval_ms do
    Application.get_env(:opake_indexer, :cursor_save_interval_ms, 5_000)
  end

  # -- Public entry point --

  @spec process_message(binary(), non_neg_integer()) :: non_neg_integer()
  def process_message(json, event_count) do
    now = DateTime.utc_now()

    {time_us, collection, payload} = Event.parse(json)

    State.bump_total()
    State.mark_event_received()
    if collection, do: State.bump_collection(collection)
    # Real consume events carry a firehose timestamp; frames without one
    # (e.g. non-commit control frames) are not consume events, so they
    # contribute no lag sample.
    if is_integer(time_us), do: ConsumeLag.record_lag(now, time_us)

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

  # -- Dispatch -------------------------------------------------------

  # Ingest gate (D6): structural validation for all versions, vocabulary
  # enforcement for known versions. A refused record is not indexed and not
  # broadcast — rejection, not deletion; it remains on the author's PDS.
  defp dispatch({:upsert_record, attrs}, _time_us, now) do
    case Validator.validate(attrs.collection, attrs.record_jsonb) do
      :ok ->
        dispatch_upsert(attrs, now)

      {:refused, reason} ->
        Logger.warning(
          "[Indexer] ingest gate refused #{attrs.collection} #{attrs.uri} " <>
            "by #{attrs.author_did}: #{inspect(reason)}"
        )

        emit_event_telemetry(attrs.collection, :upsert, :refused)
    end
  end

  defp dispatch({:delete_record, %{uri: uri}}, _time_us, now) do
    dispatch_delete(uri, now)
  end

  defp dispatch({:account_config_seen, %{did: did, op: op}}, _time_us, _now) do
    Logger.info("[Indexer] accountConfig #{op}: #{did}")

    action =
      case op do
        "create" -> :upsert
        "update" -> :upsert
        "delete" -> :delete
      end

    emit_event_telemetry(@account_config_collection, action, :ok)
  end

  defp dispatch_upsert(attrs, now) do
    case attrs.collection do
      @keyring_collection ->
        dispatch_keyring_upsert(attrs, now)

      @directory_collection ->
        dispatch_directory_upsert(attrs, now)

      @document_collection ->
        dispatch_document_upsert(attrs, now)

      @grant_collection ->
        dispatch_grant_upsert(attrs, now)

      _ ->
        :ok
    end
  end

  # -- Keyring upsert -------------------------------------------------

  defp dispatch_keyring_upsert(attrs, now) do
    workspace_id = attrs.workspace_id
    prior_uri = attrs.supersedes_uri

    if is_nil(workspace_id) do
      Logger.warning(
        "[Indexer] keyring upsert with unresolvable workspace_id: #{attrs.uri} (supersedes #{inspect(prior_uri)})"
      )

      # Persist as orphan; heals when predecessor arrives.
      _ = upsert_record(attrs, now)
      emit_event_telemetry(@keyring_collection, :upsert, :error)
    else
      Logger.info(
        "[Indexer] keyring upsert: #{attrs.uri} workspace=#{workspace_id} supersedes=#{inspect(prior_uri)}"
      )

      with :ok <-
             Authority.check_keyring_supersede(
               workspace_id,
               prior_uri,
               attrs.author_did,
               attrs.record_jsonb
             ),
           {:ok, _} <- upsert_record(attrs, now) do
        chain_outcome =
          advance_or_create_chain(
            "keyring",
            workspace_id,
            attrs.uri,
            attrs.cid,
            prior_uri,
            fn fork ->
              Broadcaster.broadcast_chain_forked(%{
                workspace_id: workspace_id,
                scope: "keyring",
                path: nil,
                your_uri: attrs.uri,
                fork_point_uri: prior_uri,
                winner_uri: fork.head_uri,
                winner_cid: fork.head_cid
              })
            end
          )

        emit_event_telemetry(@keyring_collection, :upsert, status_of_chain(chain_outcome))

        if chain_outcome == :ok do
          broadcast_record_envelope(attrs, now)
        end
      else
        {:rejected, reason} ->
          Logger.warning(
            "[Indexer] keyring authority rejection (#{reason}): #{attrs.uri} by #{attrs.author_did} in #{workspace_id}"
          )

          emit_event_telemetry(@keyring_collection, :upsert, :rejected)

        {:error, changeset} ->
          log_query_error({:error, changeset}, "keyring record upsert", attrs.uri)
          emit_event_telemetry(@keyring_collection, :upsert, :error)
      end
    end
  end

  # -- Directory upsert -----------------------------------------------

  defp dispatch_directory_upsert(attrs, now) do
    workspace_id = attrs.workspace_id
    prior_uri = attrs.supersedes_uri

    Logger.info(
      "[Indexer] directory upsert: #{attrs.uri} workspace=#{inspect(workspace_id)} supersedes=#{inspect(prior_uri)} root=#{attrs.is_workspace_root}"
    )

    prior_record = if prior_uri, do: RecordQueries.lookup(prior_uri), else: nil

    new_entries = attrs.record_jsonb["entries"] || []

    flag_check = Authority.check_workspace_root_flag(prior_record, attrs.is_workspace_root)

    auth_check =
      cond do
        # Cabinet directories — no workspace, no chain, no authority check.
        is_nil(workspace_id) ->
          :ok

        # Workspace directories — authority depends on supersede vs genesis.
        true ->
          Authority.check_directory_supersede(
            workspace_id,
            prior_uri,
            attrs.author_did,
            new_entries
          )
      end

    lineage_check = Authority.check_lineage(prior_record, attrs.record_jsonb["lineage"])

    with :ok <- flag_check,
         :ok <- lineage_check,
         :ok <- auth_check,
         {:ok, _} <- upsert_record(attrs, now) do
      chain_outcome =
        cond do
          is_nil(workspace_id) ->
            :ok

          attrs.is_workspace_root ->
            advance_or_create_chain(
              "workspace_root",
              workspace_id,
              attrs.uri,
              attrs.cid,
              prior_uri,
              fn fork ->
                Broadcaster.broadcast_chain_forked(%{
                  workspace_id: workspace_id,
                  scope: "directory",
                  path: "/",
                  your_uri: attrs.uri,
                  fork_point_uri: prior_uri,
                  winner_uri: fork.head_uri,
                  winner_cid: fork.head_cid
                })
              end
            )

          true ->
            :ok
        end

      emit_event_telemetry(@directory_collection, :upsert, status_of_chain(chain_outcome))

      if chain_outcome == :ok do
        broadcast_record_envelope(attrs, now)
      end
    else
      {:rejected, reason} ->
        Logger.warning(
          "[Indexer] directory rejection (#{reason}): #{attrs.uri} by #{attrs.author_did}"
        )

        emit_event_telemetry(@directory_collection, :upsert, :rejected)

      {:error, changeset} ->
        log_query_error({:error, changeset}, "directory record upsert", attrs.uri)
        emit_event_telemetry(@directory_collection, :upsert, :error)
    end
  end

  # -- Document / Grant upsert (no chain) -----------------------------

  defp dispatch_document_upsert(attrs, now) do
    Logger.info("[Indexer] document upsert: #{attrs.uri}")

    prior_uri = attrs.supersedes_uri
    prior_record = if prior_uri, do: RecordQueries.lookup(prior_uri), else: nil

    with :ok <- Authority.check_lineage(prior_record, attrs.record_jsonb["lineage"]),
         {:ok, _} <- upsert_record(attrs, now) do
      emit_event_telemetry(@document_collection, :upsert, :ok)
      broadcast_record_envelope(attrs, now)
    else
      {:rejected, reason} ->
        Logger.warning(
          "[Indexer] document rejection (#{reason}): #{attrs.uri} by #{attrs.author_did}"
        )

        emit_event_telemetry(@document_collection, :upsert, :rejected)

      {:error, changeset} ->
        log_query_error({:error, changeset}, "document record upsert", attrs.uri)
        emit_event_telemetry(@document_collection, :upsert, :error)
    end
  end

  defp dispatch_grant_upsert(attrs, now) do
    Logger.info("[Indexer] grant upsert: #{attrs.uri}")

    case upsert_record(attrs, now) do
      {:ok, _} ->
        emit_event_telemetry(@grant_collection, :upsert, :ok)
        broadcast_record_envelope(attrs, now)

      {:error, changeset} ->
        log_query_error({:error, changeset}, "grant record upsert", attrs.uri)
        emit_event_telemetry(@grant_collection, :upsert, :error)
    end
  end

  # -- Delete ---------------------------------------------------------

  defp dispatch_delete(uri, now) do
    case RecordQueries.lookup(uri) do
      nil ->
        # Unknown record; nothing to do beyond logging.
        Logger.info("[Indexer] delete for unknown record: #{uri}")
        emit_event_telemetry(nil, :delete, :ok)

      %{collection: @keyring_collection} = record ->
        RecordQueries.soft_delete(uri, now)
        outcome = resolve_keyring_delete(record)
        emit_event_telemetry(@keyring_collection, :delete, :ok)
        Broadcaster.broadcast_keyring_delete(record, outcome_name(outcome))

        with {:rolled_back, restored} <- outcome do
          broadcast_restored_head(restored)
        end

      %{collection: collection} = record ->
        RecordQueries.soft_delete(uri, now)
        maybe_rollback_chain(record)
        emit_event_telemetry(collection, :delete, :ok)
        broadcast_record_delete(record)
    end
  end

  # Resolves what a keyring delete means for the tracked chain. Only a
  # head delete moves anything: deleting genesis or a superseded
  # intermediate leaves the chain untouched — the genesis URI identifies
  # the workspace, not a live record. On a head delete the rollback
  # target is the newest live record for the workspace, not the
  # tombstone's `supersedes` link, which can dangle once intermediate
  # tombstones are purged. No live record left means the workspace's
  # keys are gone everywhere: tear down its tracked chains.
  defp resolve_keyring_delete(%{workspace_id: workspace_id, uri: uri})
       when is_binary(workspace_id) do
    case ChainHeadQueries.get(workspace_id, "keyring") do
      %{head_uri: ^uri} ->
        case RecordQueries.newest_live_keyring(workspace_id) do
          nil ->
            ChainHeadQueries.delete_all(workspace_id)
            :torn_down

          restored ->
            ChainHeadQueries.rollback(workspace_id, "keyring", uri, restored.uri, restored.cid)
            {:rolled_back, restored}
        end

      _ ->
        :unchanged
    end
  end

  # Orphan row (predecessor never indexed): no tracked chain exists.
  defp resolve_keyring_delete(_record), do: :unchanged

  defp outcome_name(:unchanged), do: "unchanged"
  defp outcome_name(:torn_down), do: "torn_down"
  defp outcome_name({:rolled_back, _}), do: "rolled_back"

  # A rollback changes the current member set, rotation, and metadata
  # back to the restored record's contents. Clients rebuild through the
  # ordinary upsert path, so re-emit the restored record as a normal
  # keyring:upsert — broadcast only, no dispatch re-entry.
  defp broadcast_restored_head(restored) do
    envelope = %{
      uri: restored.uri,
      record: restored.record_jsonb,
      indexedAt: DateTime.to_iso8601(restored.indexed_at)
    }

    Broadcaster.broadcast_record_upsert(restored, envelope)
  end

  defp maybe_rollback_chain(%{
         collection: @directory_collection,
         workspace_id: workspace_id,
         uri: uri,
         is_workspace_root: true
       })
       when is_binary(workspace_id) do
    case ChainHeadQueries.get(workspace_id, "workspace_root") do
      %{head_uri: ^uri} ->
        case predecessor_record(uri) do
          {:ok, pred} ->
            ChainHeadQueries.rollback(
              workspace_id,
              "workspace_root",
              uri,
              pred.uri,
              pred.cid
            )

          :none ->
            ChainHeadQueries.delete(workspace_id, "workspace_root")
        end

      _ ->
        :ok
    end
  end

  defp maybe_rollback_chain(_), do: :ok

  defp predecessor_record(uri) do
    case RecordQueries.lookup(uri) do
      %{record_jsonb: %{"supersedes" => prior}} when is_binary(prior) ->
        case RecordQueries.lookup(prior) do
          nil -> :none
          pred -> {:ok, pred}
        end

      _ ->
        :none
    end
  end

  # -- Shared chain helpers -------------------------------------------

  defp upsert_record(attrs, now) do
    RecordQueries.upsert(Map.put(attrs, :indexed_at, now))
  end

  defp advance_or_create_chain(kind, workspace_id, head_uri, head_cid, nil, _fork_cb) do
    case ChainHeadQueries.create(workspace_id, kind, head_uri, head_cid) do
      {:ok, _} ->
        :ok

      :already_exists ->
        Logger.warning(
          "[Indexer] duplicate genesis for #{kind} chain workspace=#{workspace_id}: #{head_uri}"
        )

        :error

      {:error, reason} ->
        Logger.warning("[Indexer] chain create failed: #{inspect(reason)}")
        :error
    end
  end

  defp advance_or_create_chain(kind, workspace_id, head_uri, head_cid, prior_uri, fork_cb)
       when is_binary(prior_uri) do
    case ChainHeadQueries.advance(workspace_id, kind, head_uri, head_cid, prior_uri) do
      {:ok, _} ->
        :ok

      :fork_detected ->
        case ChainHeadQueries.get(workspace_id, kind) do
          %{head_uri: winner_uri, head_cid: winner_cid} ->
            fork_cb.(%{head_uri: winner_uri, head_cid: winner_cid})

          _ ->
            :ok
        end

        Logger.warning(
          "[Indexer] #{kind} fork at #{workspace_id}: #{head_uri} tried to supersede #{prior_uri}"
        )

        :ok

      :no_chain ->
        Logger.warning(
          "[Indexer] #{kind} supersede with no chain head: workspace=#{workspace_id} uri=#{head_uri}"
        )

        :error
    end
  end

  defp status_of_chain(:ok), do: :ok
  defp status_of_chain(_), do: :error

  # -- Broadcast helpers ----------------------------------------------

  defp broadcast_record_envelope(attrs, now) do
    envelope = %{
      uri: attrs.uri,
      record: attrs.record_jsonb,
      indexedAt: DateTime.to_iso8601(now)
    }

    Broadcaster.broadcast_record_upsert(attrs, envelope)
  end

  defp broadcast_record_delete(record) do
    Broadcaster.broadcast_record_delete(record)
  end

  # -- Cursor save (time-throttled + monotonic) -----------------------

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

  # -- Misc helpers ---------------------------------------------------

  defp log_query_error({:error, reason}, label, uri) do
    Logger.warning("Failed #{label} for #{uri}: #{inspect(reason)}")
  end

  defp emit_event_telemetry(collection, action, status) do
    :telemetry.execute(
      @telemetry_prefix ++ [:event],
      %{count: 1},
      %{collection: collection, action: action, status: status}
    )
  end
end
