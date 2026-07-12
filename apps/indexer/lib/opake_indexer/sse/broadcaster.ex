defmodule OpakeIndexer.SSE.Broadcaster do
  @moduledoc """
  Emits SSE events onto Phoenix.PubSub topics.

  Two event families:

    * **Record envelopes** — `{collection}:upsert` and `{collection}:delete`.
      The payload is `{record: <verbatim PDS JSON>, indexedAt}` for upserts
      and `{uri}` for deletes — except keyring deletes, which carry
      `{uri, workspace_id, outcome}` so clients act on the resolved chain
      outcome instead of guessing from the URI. No reshaping happens here —
      the JSON that the PDS signed is the JSON we forward.

    * **Notifications** — `chain:forked`. Flat payload describing a
      detected fork race for client-side retry. Not record-shaped.

  Topics:

    * Workspace topics — every workspace-scoped record goes onto
      `Topics.workspace(workspace_id)`.
    * Personal topics — keyring upserts fan out to every member's
      personal topic so a recipient sees workspaces they belong to.
      Grants fan out to author + recipient personal topics.
    * Cabinet (no workspace_id) records go onto the author's personal
      topic.

  This module never touches the indexer DB — it's a fan-out helper.
  """

  require Logger

  alias OpakeIndexer.SSE.Topics

  @pubsub OpakeIndexer.PubSub

  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @keyring_collection "app.opake.keyring"
  @grant_collection "app.opake.grant"

  # -- Record upsert / delete -----------------------------------------

  @doc """
  Broadcast a record upsert envelope. `attrs` is the dispatch attrs
  (carries collection, workspace_id, author_did, record_jsonb); `envelope`
  is the prebuilt `{record, indexedAt}` map.
  """
  def broadcast_record_upsert(attrs, envelope) do
    event_type = "#{attrs.collection}:upsert"

    fan_out(attrs, event_type, envelope)
  rescue
    e -> Logger.warning("[Broadcaster] record upsert broadcast failed: #{inspect(e)}")
  end

  @doc """
  Broadcast a record delete tombstone. Payload is just the URI; clients
  remove the record from their local view.
  """
  def broadcast_record_delete(record) do
    event_type = "#{record.collection}:delete"
    payload = %{uri: record.uri}

    fan_out(record, event_type, payload)
  rescue
    e -> Logger.warning("[Broadcaster] record delete broadcast failed: #{inspect(e)}")
  end

  @doc """
  Broadcast a keyring delete with its resolved chain outcome.

  Payload: `%{uri, workspace_id, outcome}` where outcome is
  `"unchanged"` (deleted record was not the head), `"rolled_back"`
  (head deleted, chain rolled back — a `keyring:upsert` of the restored
  record follows), or `"torn_down"` (no live record remains). For an
  orphan row without a `workspace_id`, the tombstone's own URI stands
  in as the workspace identity.

  Fans out to the workspace topic and to the deleted record's members'
  personal topics.
  """
  def broadcast_keyring_delete(record, outcome) do
    workspace_id = record.workspace_id || record.uri
    event_type = "#{@keyring_collection}:delete"
    payload = %{uri: record.uri, workspace_id: workspace_id, outcome: outcome}

    broadcast(Topics.workspace(workspace_id), event_type, payload)

    for entry <- record.record_jsonb["members"] || [] do
      did = get_in(entry, ["wrappedKey", "did"])
      if is_binary(did), do: broadcast(Topics.personal(did), event_type, payload)
    end

    :ok
  rescue
    e -> Logger.warning("[Broadcaster] keyring delete broadcast failed: #{inspect(e)}")
  end

  @doc """
  Broadcast a `chain:forked` notification on the workspace topic.
  Payload shape:

      %{
        workspace_id, scope ("keyring" | "directory"),
        path (nullable), your_uri, fork_point_uri,
        winner_uri, winner_cid
      }
  """
  def broadcast_chain_forked(payload) do
    workspace_id = payload[:workspace_id] || payload["workspace_id"]

    if workspace_id do
      broadcast(Topics.workspace(workspace_id), "chain:forked", payload)
    end
  rescue
    e -> Logger.warning("[Broadcaster] chain:forked broadcast failed: #{inspect(e)}")
  end

  # -- Internal -------------------------------------------------------

  # Routes a record event to the correct set of topics based on
  # collection and workspace membership.
  defp fan_out(%{collection: @keyring_collection} = attrs, event_type, payload) do
    # Workspace topic — every workspace-scoped event lands here.
    if attrs.workspace_id,
      do: broadcast(Topics.workspace(attrs.workspace_id), event_type, payload)

    # Personal topics — fan out to every member's personal topic so
    # workspaces show up in their workspace list even before they
    # subscribe to the workspace topic.
    record = record_jsonb_from(attrs, payload)

    for entry <- (record["members"] || []) do
      did = get_in(entry, ["wrappedKey", "did"])
      if is_binary(did), do: broadcast(Topics.personal(did), event_type, payload)
    end
  end

  defp fan_out(%{collection: @grant_collection} = attrs, event_type, payload) do
    record = record_jsonb_from(attrs, payload)
    recipient = record["recipient"]
    author = attrs[:author_did] || record["author_did"]

    if is_binary(recipient), do: broadcast(Topics.personal(recipient), event_type, payload)
    if is_binary(author), do: broadcast(Topics.personal(author), event_type, payload)
  end

  defp fan_out(%{collection: c} = attrs, event_type, payload)
       when c in [@directory_collection, @document_collection] do
    case attrs[:workspace_id] do
      nil ->
        if did = attrs[:author_did],
          do: broadcast(Topics.personal(did), event_type, payload)

      ws ->
        broadcast(Topics.workspace(ws), event_type, payload)
    end
  end

  defp fan_out(_attrs, _event_type, _payload), do: :ok

  # Best-effort fetch of the record JSONB. For upsert events the payload
  # is `%{record: ..., indexedAt: ...}`; for delete events the payload is
  # `%{uri: ...}` and there's no record body — callers that need a JSONB
  # field (member list, recipient) can pass it via attrs[:record_jsonb].
  defp record_jsonb_from(_attrs, %{record: r}) when is_map(r), do: r
  defp record_jsonb_from(attrs, _), do: attrs[:record_jsonb] || %{}

  defp broadcast(topic, event_type, payload) do
    Phoenix.PubSub.broadcast(@pubsub, topic, {:sse_event, event_type, payload})
  end
end
