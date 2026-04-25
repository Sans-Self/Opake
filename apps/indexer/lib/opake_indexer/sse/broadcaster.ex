defmodule OpakeIndexer.SSE.Broadcaster do
  @moduledoc """
  Broadcasts indexed events to SSE subscribers via Phoenix PubSub.

  Called from the indexer after each successful DB write. Fire-and-forget —
  broadcast failures are logged at warning level but never propagate.

  Formatting uses bracket-access (`attrs[:key]`) because the indexer passes
  raw event attrs (plain maps from the firehose parser), not Ecto schema
  structs. TreeHelpers formatters use dot access and require fields like
  `indexed_at` that only exist on DB records — they can't be reused here.
  """

  require Logger

  alias OpakeIndexer.SSE.Topics
  alias OpakeIndexerWeb.TreeHelpers

  @pubsub OpakeIndexer.PubSub

  # -- Public API --

  def broadcast_directory(attrs, action) do
    payload = if action == :upsert, do: format_directory(attrs), else: %{directory_uri: get(attrs, :directory_uri)}
    broadcast_owned(attrs, "directory", action, payload)
  rescue
    e -> Logger.warning("[Broadcaster] directory broadcast failed: #{inspect(e)}")
  end

  def broadcast_document(attrs, action) do
    payload = if action == :upsert, do: format_document(attrs), else: %{document_uri: get(attrs, :document_uri)}
    broadcast_owned(attrs, "document", action, payload)
  rescue
    e -> Logger.warning("[Broadcaster] document broadcast failed: #{inspect(e)}")
  end

  def broadcast_keyring(attrs, action) do
    uri = get(attrs, :uri)
    owner = get(attrs, :owner_did)
    payload = if action == :upsert, do: format_keyring(attrs), else: %{uri: uri}
    event_type = "keyring:#{action}"

    # Broadcast to the workspace topic (existing subscribers)
    if uri, do: broadcast(Topics.workspace(uri), event_type, payload)
    # Broadcast to the owner's personal topic
    if owner, do: broadcast(Topics.personal(owner), event_type, payload)

    # Broadcast to each member's personal topic so newly-added members
    # can discover the keyring and subscribe to its workspace topic.
    if action == :upsert do
      for entry <- get(attrs, :member_entries) || [] do
        did = entry[:did] || entry["did"]
        if did && did != owner, do: broadcast(Topics.personal(did), event_type, payload)
      end
    end
  rescue
    e -> Logger.warning("[Broadcaster] keyring broadcast failed: #{inspect(e)}")
  end

  def broadcast_grant(attrs, action) do
    payload =
      case action do
        :upsert -> TreeHelpers.format_grant(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    if owner = get(attrs, :owner_did), do: broadcast(Topics.personal(owner), "grant:#{action}", payload)
    maybe_broadcast_recipient(attrs, payload, action)
  rescue
    e -> Logger.warning("[Broadcaster] grant broadcast failed: #{inspect(e)}")
  end

  def broadcast_directory_update(attrs, action) do
    payload =
      case action do
        :upsert -> format_proposal(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    if kr = get(attrs, :keyring_uri), do: broadcast(Topics.workspace(kr), "directory_update:#{action}", payload)
  rescue
    e -> Logger.warning("[Broadcaster] directory_update broadcast failed: #{inspect(e)}")
  end

  def broadcast_keyring_update(attrs, action) do
    payload =
      case action do
        :upsert -> format_keyring_proposal(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    if kr = get(attrs, :keyring_uri), do: broadcast(Topics.workspace(kr), "keyring_update:#{action}", payload)
  rescue
    e -> Logger.warning("[Broadcaster] keyring_update broadcast failed: #{inspect(e)}")
  end

  def broadcast_document_update(attrs, action) do
    payload =
      case action do
        :upsert -> format_document_proposal(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    event_type = "document_update:#{action}"

    # `app.opake.documentUpdate` carries no `keyring` field in the
    # lexicon, so the indexer injects `keyring_uri` at dispatch time
    # via a JOIN through the documents table. When the lookup
    # succeeds we broadcast on the workspace topic where the owner
    # (and every other member) is subscribed.
    #
    # When the lookup fails — cabinet documents (ill-formed for this
    # event type, since cabinets have single owners) or a backfill
    # edge case where the proposal is indexed before its parent
    # document — we drop the event silently. The proposal row is
    # still written to the DB, so the owner's next
    # `sync_workspace_by_uri` call picks it up from the proposal
    # store whenever that fires. Nothing is lost; only the real-time
    # hop is skipped.
    case get(attrs, :keyring_uri) do
      nil ->
        Logger.debug("[Broadcaster] document_update: no keyring for #{get(attrs, :uri)}, dropping")
        :ok

      kr ->
        broadcast(Topics.workspace(kr), event_type, payload)
    end
  rescue
    e -> Logger.warning("[Broadcaster] document_update broadcast failed: #{inspect(e)}")
  end

  # -- Internal --

  defp broadcast_owned(attrs, prefix, action, payload) do
    event_type = "#{prefix}:#{action}"

    case get(attrs, :keyring_uri) do
      nil -> if did = get(attrs, :owner_did), do: broadcast(Topics.personal(did), event_type, payload)
      kr -> broadcast(Topics.workspace(kr), event_type, payload)
    end
  end

  defp broadcast(topic, event_type, payload) do
    Phoenix.PubSub.broadcast(@pubsub, topic, {:sse_event, event_type, payload})
  end

  defp maybe_broadcast_recipient(attrs, payload, action) do
    if recipient = get(attrs, :recipient_did) do
      broadcast(Topics.personal(recipient), "grant:#{action}", payload)
    end
  end

  defp get(attrs, key), do: attrs[key]

  # -- Formatters (bracket-access safe for raw indexer attrs) --

  defp format_directory(attrs) do
    %{directory_uri: get(attrs, :directory_uri), owner_did: get(attrs, :owner_did), entries: get(attrs, :entries) || []}
    |> TreeHelpers.maybe_put(:keyring_uri, get(attrs, :keyring_uri))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:key_wrapping, get(attrs, :key_wrapping))
  end

  defp format_document(attrs) do
    %{document_uri: get(attrs, :document_uri), owner_did: get(attrs, :owner_did)}
    |> TreeHelpers.maybe_put(:keyring_uri, get(attrs, :keyring_uri))
    |> TreeHelpers.maybe_put(:rotation, get(attrs, :rotation))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:encryption, get(attrs, :encryption))
    |> TreeHelpers.maybe_put(:blob_ref, get(attrs, :blob_ref))
  end

  defp format_keyring(attrs) do
    %{uri: get(attrs, :uri), owner_did: get(attrs, :owner_did), rotation: get(attrs, :rotation), member_entries: get(attrs, :member_entries) || []}
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:created_at, get(attrs, :created_at))
  end

  # Proposal formatters — safe bracket-access versions of TreeHelpers formatters
  # which use dot access and require DB schema fields like indexed_at.

  defp format_proposal(attrs) do
    %{uri: get(attrs, :uri), author_did: get(attrs, :author_did), action_type: get(attrs, :action_type)}
    |> TreeHelpers.maybe_put(:keyring_uri, get(attrs, :keyring_uri))
    |> TreeHelpers.maybe_put(:directory_uri, get(attrs, :directory_uri))
    |> TreeHelpers.maybe_put(:entry_uri, get(attrs, :entry_uri))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:source_directory_uri, get(attrs, :source_directory_uri))
    |> TreeHelpers.maybe_put(:target_directory_uri, get(attrs, :target_directory_uri))
    |> TreeHelpers.maybe_put(:parent_directory_uri, get(attrs, :parent_directory_uri))
  end

  defp format_keyring_proposal(attrs) do
    %{uri: get(attrs, :uri), author_did: get(attrs, :author_did), action_type: get(attrs, :action_type)}
    |> TreeHelpers.maybe_put(:keyring_uri, get(attrs, :keyring_uri))
    |> TreeHelpers.maybe_put(:member_did, get(attrs, :member_did))
    |> TreeHelpers.maybe_put(:member_public_key, get(attrs, :member_public_key))
    |> TreeHelpers.maybe_put(:role, get(attrs, :role))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
  end

  defp format_document_proposal(attrs) do
    %{uri: get(attrs, :uri), document_uri: get(attrs, :document_uri), author_did: get(attrs, :author_did)}
    |> TreeHelpers.maybe_put(:keyring_uri, get(attrs, :keyring_uri))
    |> TreeHelpers.maybe_put(:supersedes_uri, get(attrs, :supersedes_uri))
  end
end
