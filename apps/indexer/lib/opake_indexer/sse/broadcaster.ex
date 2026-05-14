defmodule OpakeIndexer.SSE.Broadcaster do
  @moduledoc """
  Broadcasts indexed events to SSE subscribers via Phoenix PubSub.

  Called from the indexer after each successful DB write. Fire-and-forget —
  broadcast failures are logged at warning level but never propagate.

  Formatting uses bracket-access (`attrs[:key]`) because the indexer passes
  raw event attrs (plain maps from the firehose parser), not Ecto schema
  structs.

  Federation rewrite: the `*_update` broadcasts are gone with the proposal
  lexicons. Member writes route through curatorial supersedes on the same
  record types (directory / keyring), and forks surface via
  `chain:forked`.
  """

  require Logger

  alias OpakeIndexer.SSE.Topics
  alias OpakeIndexerWeb.TreeHelpers

  @pubsub OpakeIndexer.PubSub

  # -- Public API --

  def broadcast_directory(attrs, action) do
    payload =
      case action do
        :upsert -> format_directory(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    broadcast_owned(attrs, "directory", action, payload)
  rescue
    e -> Logger.warning("[Broadcaster] directory broadcast failed: #{inspect(e)}")
  end

  def broadcast_document(attrs, action) do
    payload =
      case action do
        :upsert -> format_document(attrs)
        :delete -> %{uri: get(attrs, :uri)}
      end

    broadcast_owned(attrs, "document", action, payload)
  rescue
    e -> Logger.warning("[Broadcaster] document broadcast failed: #{inspect(e)}")
  end

  def broadcast_keyring(attrs, action) do
    uri = get(attrs, :uri)
    workspace_id = get(attrs, :workspace_id)

    payload =
      case action do
        :upsert -> format_keyring(attrs)
        :delete -> %{uri: uri}
      end

    event_type = "keyring:#{action}"

    if workspace_id, do: broadcast(Topics.workspace(workspace_id), event_type, payload)

    if action == :upsert do
      for entry <- get(attrs, :member_entries) || [] do
        did = entry[:did] || entry["did"]
        if is_binary(did), do: broadcast(Topics.personal(did), event_type, payload)
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

    if author = get(attrs, :author_did),
      do: broadcast(Topics.personal(author), "grant:#{action}", payload)

    maybe_broadcast_recipient(attrs, payload, action)
  rescue
    e -> Logger.warning("[Broadcaster] grant broadcast failed: #{inspect(e)}")
  end

  @doc """
  Announce a chain fork to the workspace topic. `payload` should be a map
  with workspace_id, scope ("keyring" | "directory"), path (nullable),
  your_uri (the supersede that lost), fork_point_uri (the URI both
  supersedes targeted), and the winning head's URI + CID.
  """
  def broadcast_chain_forked(payload) do
    workspace_id = get(payload, :workspace_id)

    if workspace_id do
      broadcast(Topics.workspace(workspace_id), "chain:forked", payload)
    end
  rescue
    e -> Logger.warning("[Broadcaster] chain:forked broadcast failed: #{inspect(e)}")
  end

  # -- Internal --

  defp broadcast_owned(attrs, prefix, action, payload) do
    event_type = "#{prefix}:#{action}"

    case get(attrs, :workspace_id) do
      nil ->
        if did = get(attrs, :author_did),
          do: broadcast(Topics.personal(did), event_type, payload)

      workspace_id ->
        broadcast(Topics.workspace(workspace_id), event_type, payload)
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
    %{
      uri: get(attrs, :uri),
      author_did: get(attrs, :author_did),
      entries: get(attrs, :entries_json) || []
    }
    |> TreeHelpers.maybe_put(:workspace_id, get(attrs, :workspace_id))
    |> TreeHelpers.maybe_put(:chain_genesis_uri, get(attrs, :chain_genesis_uri))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:key_wrapping, get(attrs, :key_wrapping))
    |> TreeHelpers.maybe_put(:supersedes_uri, get(attrs, :supersedes_uri))
    |> TreeHelpers.maybe_put(:modified_at, get(attrs, :modified_at))
  end

  defp format_document(attrs) do
    %{
      uri: get(attrs, :uri),
      author_did: get(attrs, :author_did)
    }
    |> TreeHelpers.maybe_put(:workspace_id, get(attrs, :workspace_id))
    |> TreeHelpers.maybe_put(:rotation, get(attrs, :rotation))
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:encryption, get(attrs, :encryption))
    |> TreeHelpers.maybe_put(:blob_ref, get(attrs, :blob_ref))
    |> TreeHelpers.maybe_put(:supersedes_uri, get(attrs, :supersedes_uri))
    |> TreeHelpers.maybe_put(:modified_at, get(attrs, :modified_at))
  end

  defp format_keyring(attrs) do
    %{
      uri: get(attrs, :uri),
      workspace_id: get(attrs, :workspace_id),
      rotation: get(attrs, :rotation),
      member_entries: get(attrs, :member_entries) || []
    }
    |> TreeHelpers.maybe_put(:encrypted_metadata, get(attrs, :encrypted_metadata))
    |> TreeHelpers.maybe_put(:supersedes_uri, get(attrs, :supersedes_uri))
    |> TreeHelpers.maybe_put(:created_at, get(attrs, :created_at))
    |> TreeHelpers.maybe_put(:modified_at, get(attrs, :modified_at))
  end
end
