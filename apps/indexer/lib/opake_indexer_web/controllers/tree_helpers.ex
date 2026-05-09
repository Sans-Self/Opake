defmodule OpakeIndexerWeb.TreeHelpers do
  @moduledoc """
  Shared helpers for tree/sync responses across workspace and cabinet controllers.
  Serializes directory and document schemas to snake_case JSON.
  """

  @spec format_tree_response([map()], [map()], DateTime.t(), [map()]) :: map()
  def format_tree_response(directories, documents, server_time, proposals \\ []) do
    base = %{
      directories: Enum.map(directories, &format_directory/1),
      documents: Enum.map(documents, &format_document/1),
      server_time: DateTime.to_iso8601(server_time)
    }

    if proposals == [] do
      base
    else
      Map.put(base, :proposals, Enum.map(proposals, &format_proposal/1))
    end
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
      directory_uri: dir.directory_uri,
      owner_did: dir.owner_did,
      entries: dir.entries || [],
      indexed_at: format_datetime(dir.indexed_at)
    }

    base
    |> maybe_put(:keyring_uri, dir.keyring_uri)
    |> maybe_put(:encrypted_metadata, dir.encrypted_metadata)
    |> maybe_put(:key_wrapping, dir.key_wrapping)
    |> maybe_put(:modified_at, dir.modified_at)
    |> maybe_put(:deleted_at, format_datetime(dir.deleted_at))
  end

  def format_document(doc) do
    base = %{
      document_uri: doc.document_uri,
      owner_did: doc.owner_did,
      indexed_at: format_datetime(doc.indexed_at)
    }

    base
    |> maybe_put(:keyring_uri, doc.keyring_uri)
    |> maybe_put(:rotation, doc.rotation)
    |> maybe_put(:encrypted_metadata, doc.encrypted_metadata)
    |> maybe_put(:encryption, doc.encryption)
    |> maybe_put(:blob_ref, doc.blob_ref)
    |> maybe_put(:modified_at, doc.modified_at)
    |> maybe_put(:deleted_at, format_datetime(doc.deleted_at))
  end

  def format_proposal(update) do
    format_update_base(update)
    |> maybe_put(:directory_uri, update.directory_uri)
    |> maybe_put(:entry_uri, update.entry_uri)
    |> maybe_put(:encrypted_metadata, update.encrypted_metadata)
    |> maybe_put(:source_directory_uri, update.source_directory_uri)
    |> maybe_put(:target_directory_uri, update.target_directory_uri)
    |> maybe_put(:parent_directory_uri, update.parent_directory_uri)
  end

  def format_keyring_proposal(update) do
    format_update_base(update)
    |> maybe_put(:member_did, update.member_did)
    |> maybe_put(:member_public_key, encode_binary(update.member_public_key))
    |> maybe_put(:role, update.role)
    |> maybe_put(:encrypted_metadata, update.encrypted_metadata)
  end

  def format_document_proposal(update) do
    %{
      uri: update.uri,
      author_did: update.author_did,
      indexed_at: format_datetime(update.indexed_at)
    }
    |> maybe_put(:document_uri, update.document_uri)
  end

  def format_grant(grant) do
    %{
      uri: grant[:uri] || grant.uri,
      owner_did: grant[:owner_did] || grant.owner_did,
      document_uri: grant[:document_uri] || grant.document_uri,
      created_at: grant[:created_at] || grant.created_at
    }
    |> maybe_put(:recipient_did, grant[:recipient_did])
  end

  defp format_update_base(update) do
    %{
      uri: update.uri,
      author_did: update.author_did,
      action_type: update.action_type,
      indexed_at: format_datetime(update.indexed_at)
    }
  end

  defp encode_binary(nil), do: nil
  defp encode_binary(bin) when is_binary(bin), do: Base.encode64(bin)

  def format_datetime(nil), do: nil
  def format_datetime(%DateTime{} = dt), do: DateTime.to_iso8601(dt)

  def maybe_put(map, _key, nil), do: map
  def maybe_put(map, key, value), do: Map.put(map, key, value)
end
