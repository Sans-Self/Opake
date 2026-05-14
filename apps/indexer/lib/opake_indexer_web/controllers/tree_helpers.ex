defmodule OpakeIndexerWeb.TreeHelpers do
  @moduledoc """
  Shared helpers for tree/sync responses across workspace and cabinet
  controllers. Serializes the federation-era directory/document schemas
  to snake_case JSON.
  """

  @spec format_tree_response([map()], [map()], DateTime.t()) :: map()
  def format_tree_response(directories, documents, server_time) do
    %{
      directories: Enum.map(directories, &format_directory/1),
      documents: Enum.map(documents, &format_document/1),
      server_time: DateTime.to_iso8601(server_time)
    }
  end

  @spec parse_since(map()) :: {:ok, DateTime.t()} | {:error, String.t()}
  def parse_since(%{"since" => since_str}) when is_binary(since_str) do
    case DateTime.from_iso8601(since_str) do
      {:ok, dt, _} -> {:ok, dt}
      {:error, _} -> {:error, "invalid since timestamp"}
    end
  end

  def parse_since(_), do: {:error, "since parameter is required"}

  def format_directory(dir) do
    base = %{
      uri: dir.uri,
      author_did: dir.author_did,
      entries: dir.entries_json || [],
      indexed_at: format_datetime(dir.indexed_at)
    }

    base
    |> maybe_put(:workspace_id, dir.workspace_id)
    |> maybe_put(:chain_genesis_uri, dir.chain_genesis_uri)
    |> maybe_put(:encrypted_metadata, dir.encrypted_metadata)
    |> maybe_put(:key_wrapping, dir.key_wrapping)
    |> maybe_put(:supersedes_uri, dir.supersedes_uri)
    |> maybe_put(:modified_at, dir.modified_at)
    |> maybe_put(:deleted_at, format_datetime(dir.deleted_at))
  end

  def format_document(doc) do
    base = %{
      uri: doc.uri,
      author_did: doc.author_did,
      indexed_at: format_datetime(doc.indexed_at)
    }

    base
    |> maybe_put(:workspace_id, doc.workspace_id)
    |> maybe_put(:rotation, doc.rotation)
    |> maybe_put(:encrypted_metadata, doc.encrypted_metadata)
    |> maybe_put(:encryption, doc.encryption)
    |> maybe_put(:blob_ref, doc.blob_ref)
    |> maybe_put(:supersedes_uri, doc.supersedes_uri)
    |> maybe_put(:modified_at, doc.modified_at)
    |> maybe_put(:deleted_at, format_datetime(doc.deleted_at))
  end

  def format_grant(grant) do
    %{
      uri: grant[:uri] || grant.uri,
      author_did: grant[:author_did] || grant.author_did,
      document_uri: grant[:document_uri] || grant.document_uri,
      created_at: grant[:created_at] || grant.created_at
    }
    |> maybe_put(:recipient_did, grant[:recipient_did])
  end

  def format_datetime(nil), do: nil
  def format_datetime(%DateTime{} = dt), do: DateTime.to_iso8601(dt)

  def maybe_put(map, _key, nil), do: map
  def maybe_put(map, key, value), do: Map.put(map, key, value)
end
