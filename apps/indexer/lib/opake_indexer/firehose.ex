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

  ## Chain dispatch

  Keyrings and directory-root records participate in supersede chains. On
  upsert the dispatch decides one of:

    * **Genesis** — no `supersedes` field. Create the chain head row.
    * **Advance** — `supersedes` matches the current head URI. Compare-
      and-set the head row (`*ChainQueries.advance/4`), refresh
      membership / root pointer.
    * **Fork** — `supersedes` points at a non-head URI. Persist the
      record but don't advance; broadcast a `chain:forked` SSE event so
      clients can retry their write.
    * **Orphan** — `supersedes` points at a record the indexer hasn't
      seen. Persist the record; no chain action. Heals naturally when
      the missing predecessor arrives.

  Authority validation (`OpakeIndexer.Authority`) runs *before* chain
  advancement on every supersede.

  Documents don't drive chains — the parent directory's listing entry is
  the canonical pointer. Documents carry `supersedes` as a history
  annotation only.

  ## Chain head bookkeeping

  `keyring_chains` and `workspace_roots` are hand-rolled materialized
  views maintained inline by this dispatch. They will drift if any code
  path writes to `keyrings` or workspace-root `directories` without
  going through here. See `OpakeIndexer.Queries.KeyringChainQueries`
  module docs for the full discipline contract.
  """

  require Logger

  alias OpakeIndexer.Authority
  alias OpakeIndexer.Jetstream.Event
  alias OpakeIndexer.Firehose.State

  alias OpakeIndexer.Queries.{
    CursorQueries,
    DirectoryQueries,
    DocumentQueries,
    GrantQueries,
    KeyringChainQueries,
    KeyringQueries,
    WorkspaceRootQueries
  }

  alias OpakeIndexer.SSE.Broadcaster

  @telemetry_prefix [:opake_indexer, :indexer]

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

  defp dispatch({:upsert_grant, attrs}, _time_us, now) do
    Logger.info(
      "[Indexer] grant upsert: #{attrs.uri} (author=#{attrs.author_did}, recipient=#{attrs.recipient_did})"
    )

    result =
      GrantQueries.upsert_grant(%{
        uri: attrs.uri,
        author_did: attrs.author_did,
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

    parties = GrantQueries.grant_parties(uri)
    GrantQueries.delete_grant(uri)
    emit_event_telemetry("app.opake.grant", :delete, :ok)

    attrs =
      case parties do
        {author_did, recipient_did} ->
          %{uri: uri, author_did: author_did, recipient_did: recipient_did}

        nil ->
          %{uri: uri}
      end

    Broadcaster.broadcast_grant(attrs, :delete)
  end

  # -- Keyring chain dispatch --

  defp dispatch({:upsert_keyring, attrs}, _time_us, _now) do
    workspace_id = resolve_workspace_id_keyring(attrs)

    if is_nil(workspace_id) do
      Logger.warning(
        "[Indexer] keyring upsert with unresolvable workspace_id: #{attrs.uri} (supersedes #{inspect(attrs.supersedes_uri)})"
      )

      # Persist the record so it can be picked up later when the chain heals.
      KeyringQueries.upsert_keyring_record(attrs)
      emit_event_telemetry("app.opake.keyring", :upsert, :error)
    else
      attrs_with_workspace = Map.put(attrs, :workspace_id, workspace_id)

      Logger.info(
        "[Indexer] keyring upsert: #{attrs.uri} workspace=#{workspace_id} supersedes=#{inspect(attrs.supersedes_uri)} members=#{length(attrs.member_entries)}"
      )

      record_result = KeyringQueries.upsert_keyring_record(attrs_with_workspace)
      log_query_error(record_result, "keyring record upsert", attrs.uri)

      chain_outcome =
        case attrs.supersedes_uri do
          nil ->
            handle_keyring_genesis(workspace_id, attrs)

          prior_uri ->
            handle_keyring_supersede(workspace_id, prior_uri, attrs)
        end

      emit_event_telemetry(
        "app.opake.keyring",
        :upsert,
        worst_status([record_result, chain_outcome])
      )

      if chain_outcome == :ok do
        Broadcaster.broadcast_keyring(attrs_with_workspace, :upsert)
      end
    end
  end

  defp dispatch({:delete_keyring, %{uri: uri}}, _time_us, _now) do
    Logger.info("[Indexer] keyring delete: #{uri}")

    # Explicit keyring deletion is rare — supersedes are the normal mutation
    # path. When it happens we tear down the whole workspace's indexed state.
    case KeyringQueries.lookup(uri) do
      nil ->
        :ok

      %{workspace_id: workspace_id} when is_binary(workspace_id) ->
        KeyringQueries.delete_workspace(workspace_id)
        KeyringChainQueries.delete(workspace_id)
        WorkspaceRootQueries.delete(workspace_id)

      _ ->
        :ok
    end

    emit_event_telemetry("app.opake.keyring", :delete, :ok)
    Broadcaster.broadcast_keyring(%{uri: uri}, :delete)
  end

  # -- Directory chain dispatch --

  defp dispatch({:upsert_directory, attrs}, _time_us, now) do
    workspace_id = resolve_workspace_id_directory(attrs)
    chain_genesis_uri = resolve_directory_chain_genesis(attrs)

    Logger.info(
      "[Indexer] directory upsert: #{attrs.uri} workspace=#{inspect(workspace_id)} supersedes=#{inspect(attrs.supersedes_uri)}"
    )

    full_attrs = %{
      uri: attrs.uri,
      workspace_id: workspace_id,
      chain_genesis_uri: chain_genesis_uri,
      author_did: attrs.author_did,
      entries_json: attrs.entries_json,
      encrypted_metadata: attrs.encrypted_metadata,
      key_wrapping: attrs.key_wrapping,
      supersedes_uri: attrs.supersedes_uri,
      modified_at: attrs[:modified_at],
      deleted_at: nil,
      indexed_at: now
    }

    record_result = DirectoryQueries.upsert_directory(full_attrs)
    log_query_error(record_result, "directory upsert", attrs.uri)

    chain_outcome =
      if workspace_id && workspace_root_chain?(workspace_id, chain_genesis_uri) do
        dispatch_workspace_root(workspace_id, attrs)
      else
        :ok
      end

    emit_event_telemetry(
      "app.opake.directory",
      :upsert,
      worst_status([record_result, chain_outcome])
    )

    if chain_outcome == :ok do
      Broadcaster.broadcast_directory(full_attrs, :upsert)
    end
  end

  defp dispatch({:delete_directory, %{uri: uri}}, _time_us, now) do
    Logger.info("[Indexer] directory delete (soft): #{uri}")
    DirectoryQueries.soft_delete_directory(uri, now)
    emit_event_telemetry("app.opake.directory", :delete, :ok)
    Broadcaster.broadcast_directory(%{uri: uri}, :delete)
  end

  # -- Document dispatch (no chain advancement) --

  defp dispatch({:upsert_document, attrs}, _time_us, now) do
    Logger.info("[Indexer] document upsert: #{attrs.uri}")

    full_attrs = %{
      uri: attrs.uri,
      workspace_id: attrs[:workspace_id],
      author_did: attrs.author_did,
      rotation: attrs.rotation,
      encrypted_metadata: attrs.encrypted_metadata,
      encryption: attrs.encryption,
      blob_ref: attrs.blob_ref,
      supersedes_uri: attrs[:supersedes_uri],
      modified_at: attrs[:modified_at],
      deleted_at: nil,
      indexed_at: now
    }

    result = DocumentQueries.upsert_document(full_attrs)
    log_query_error(result, "document upsert", attrs.uri)
    emit_event_telemetry("app.opake.document", :upsert, status_of(result))
    Broadcaster.broadcast_document(full_attrs, :upsert)
  end

  defp dispatch({:delete_document, %{uri: uri}}, _time_us, now) do
    Logger.info("[Indexer] document delete (soft): #{uri}")
    DocumentQueries.soft_delete_document(uri, now)
    emit_event_telemetry("app.opake.document", :delete, :ok)
    Broadcaster.broadcast_document(%{uri: uri}, :delete)
  end

  # -- Heartbeat --

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

  # -- Chain helpers --

  defp handle_keyring_genesis(workspace_id, attrs) do
    case KeyringChainQueries.create(workspace_id, attrs.uri, attrs.cid || "") do
      {:ok, _} ->
        KeyringQueries.replace_members(workspace_id, attrs.member_entries)
        :ok

      :already_exists ->
        Logger.warning(
          "[Indexer] duplicate genesis keyring for workspace #{workspace_id}: #{attrs.uri}"
        )

        :error

      {:error, reason} ->
        Logger.warning("[Indexer] keyring chain create failed: #{inspect(reason)}")
        :error
    end
  end

  defp handle_keyring_supersede(workspace_id, prior_uri, attrs) do
    case Authority.check_keyring_supersede(workspace_id, prior_uri, attrs.author_did) do
      :ok ->
        case KeyringChainQueries.advance(workspace_id, attrs.uri, attrs.cid || "", prior_uri) do
          {:ok, _} ->
            KeyringQueries.replace_members(workspace_id, attrs.member_entries)
            :ok

          :fork_detected ->
            current = KeyringChainQueries.get(workspace_id)

            Broadcaster.broadcast_chain_forked(%{
              workspace_id: workspace_id,
              scope: "keyring",
              path: nil,
              your_uri: attrs.uri,
              fork_point_uri: prior_uri,
              winner_uri: current.head_uri,
              winner_cid: current.head_cid
            })

            Logger.warning(
              "[Indexer] keyring fork at #{workspace_id}: attempted #{attrs.uri} (supersedes #{prior_uri}) but head is #{current.head_uri}"
            )

            :ok

          :no_chain ->
            Logger.warning(
              "[Indexer] keyring supersede with no chain head: workspace=#{workspace_id} uri=#{attrs.uri}"
            )

            :error
        end

      {:rejected, reason} ->
        Logger.warning(
          "[Indexer] keyring authority rejection (#{reason}): #{attrs.uri} by #{attrs.author_did} in #{workspace_id}"
        )

        :error
    end
  end

  defp dispatch_workspace_root(workspace_id, attrs) do
    case attrs.supersedes_uri do
      nil ->
        case WorkspaceRootQueries.create(workspace_id, attrs.uri, attrs.cid || "") do
          {:ok, _} -> :ok
          :already_exists -> :error
          {:error, reason} ->
            Logger.warning("[Indexer] workspace root create failed: #{inspect(reason)}")
            :error
        end

      prior_uri ->
        case Authority.check_directory_supersede(
               workspace_id,
               prior_uri,
               attrs.author_did,
               attrs.entries_json
             ) do
          :ok ->
            case WorkspaceRootQueries.advance(
                   workspace_id,
                   attrs.uri,
                   attrs.cid || "",
                   prior_uri
                 ) do
              {:ok, _} ->
                :ok

              :fork_detected ->
                current = WorkspaceRootQueries.get(workspace_id)

                Broadcaster.broadcast_chain_forked(%{
                  workspace_id: workspace_id,
                  scope: "directory",
                  path: "/",
                  your_uri: attrs.uri,
                  fork_point_uri: prior_uri,
                  winner_uri: current.head_uri,
                  winner_cid: current.head_cid
                })

                Logger.warning(
                  "[Indexer] workspace root fork at #{workspace_id}: attempted #{attrs.uri} but head is #{current.head_uri}"
                )

                :ok

              :no_chain ->
                Logger.warning(
                  "[Indexer] workspace root supersede with no chain: workspace=#{workspace_id} uri=#{attrs.uri}"
                )

                :error
            end

          {:rejected, reason} ->
            Logger.warning(
              "[Indexer] directory authority rejection (#{reason}): #{attrs.uri} by #{attrs.author_did} in #{workspace_id}"
            )

            :error
        end
    end
  end

  # -- Workspace ID resolution --

  defp resolve_workspace_id_keyring(%{uri: uri, workspace_id: nil, supersedes_uri: nil}), do: uri

  defp resolve_workspace_id_keyring(%{workspace_id: workspace_id})
       when is_binary(workspace_id),
       do: workspace_id

  defp resolve_workspace_id_keyring(%{supersedes_uri: prior_uri}) when is_binary(prior_uri) do
    # Fall back to walking via the indexed predecessor.
    case KeyringQueries.lookup(prior_uri) do
      %{workspace_id: workspace_id} when is_binary(workspace_id) -> workspace_id
      _ -> nil
    end
  end

  defp resolve_workspace_id_keyring(_), do: nil

  defp resolve_workspace_id_directory(%{workspace_id: workspace_id})
       when is_binary(workspace_id),
       do: workspace_id

  defp resolve_workspace_id_directory(%{keyring_uri: keyring_uri}) when is_binary(keyring_uri) do
    case KeyringQueries.lookup(keyring_uri) do
      %{workspace_id: workspace_id} when is_binary(workspace_id) -> workspace_id
      _ -> nil
    end
  end

  defp resolve_workspace_id_directory(_), do: nil

  defp resolve_directory_chain_genesis(%{supersedes_uri: nil, uri: uri}), do: uri

  defp resolve_directory_chain_genesis(%{supersedes_uri: prior_uri, uri: uri})
       when is_binary(prior_uri) do
    case DirectoryQueries.lookup(prior_uri) do
      %{chain_genesis_uri: genesis} when is_binary(genesis) -> genesis
      _ -> uri
    end
  end

  defp resolve_directory_chain_genesis(%{uri: uri}), do: uri

  # The workspace root chain's genesis URI is at://<creator>/app.opake.directory/ws-{keyring_rkey}
  # where keyring_rkey is the genesis keyring's rkey (extracted from workspace_id).
  defp workspace_root_chain?(workspace_id, chain_genesis_uri)
       when is_binary(workspace_id) and is_binary(chain_genesis_uri) do
    with {:ok, kr_rkey} <- extract_rkey(workspace_id),
         {:ok, dir_rkey} <- extract_rkey(chain_genesis_uri) do
      dir_rkey == "ws-" <> kr_rkey
    else
      _ -> false
    end
  end

  defp workspace_root_chain?(_, _), do: false

  defp extract_rkey(uri) do
    case String.split(uri, "/") do
      parts when length(parts) >= 5 -> {:ok, List.last(parts)}
      _ -> :error
    end
  end

  # -- Cursor save (time-throttled + monotonic) --

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
    if Enum.any?(results, fn r -> match?({:error, _}, r) or r == :error end),
      do: :error,
      else: :ok
  end

  defp emit_event_telemetry(collection, action, status) do
    :telemetry.execute(
      @telemetry_prefix ++ [:event],
      %{count: 1},
      %{collection: collection, action: action, status: status}
    )
  end
end
