defmodule OpakeIndexer.Jetstream.Event do
  @moduledoc """
  Parses Jetstream JSON messages into firehose dispatch tuples.

  Returns `{time_us, collection, payload}` where `payload` is one of:

    * `{:upsert_record, attrs}` — a create/update of a known
      `at.opake.*` collection. `attrs.record_jsonb` is the verbatim
      on-PDS record JSON (camelCase, no field reshaping); structural
      columns (workspace_id, supersedes_uri, is_workspace_root) are
      extracted from `record_jsonb` for index speed but every projection
      must agree with the JSONB.
    * `{:delete_record, %{uri: uri}}` — a delete of any indexed collection.
    * `{:account_config_seen, %{did, op}}` — heartbeat-only.
    * `:ignore` — unhandled message.

  No more per-record `parse_*_upsert` helpers — one path for every
  collection. The record's identity is just its at-uri; its content is
  the JSONB payload.
  """

  @grant_collection "at.opake.grant"
  @keyring_collection "at.opake.keyring"
  @directory_collection "at.opake.directory"
  @document_collection "at.opake.document"
  @account_config_collection "at.opake.accountConfig"

  @indexed_collections [
    @grant_collection,
    @keyring_collection,
    @directory_collection,
    @document_collection
  ]

  @type event_payload ::
          {:upsert_record, map()}
          | {:delete_record, map()}
          | {:account_config_seen, map()}
          | :ignore

  @type result ::
          {time_us :: integer() | nil, collection :: String.t() | nil, event_payload()}

  @spec parse(binary()) :: result()
  def parse(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, payload} ->
        time_us = extract_time_us(payload)
        {collection, parsed} = parse_payload(payload)
        {time_us, collection, parsed}

      {:error, _} ->
        {nil, nil, :ignore}
    end
  end

  # -- Internal --------------------------------------------------------

  defp extract_time_us(%{"time_us" => t}) when is_integer(t), do: t
  defp extract_time_us(_), do: nil

  defp parse_payload(%{"kind" => "commit", "did" => did, "commit" => commit}) do
    parse_commit(did, commit)
  end

  defp parse_payload(_), do: {nil, :ignore}

  defp parse_commit(
         did,
         %{"operation" => operation, "collection" => collection, "rkey" => rkey} = commit
       )
       when is_binary(collection) do
    uri = "at://#{did}/#{collection}/#{rkey}"
    parsed = parse_commit_op(uri, did, collection, operation, commit)
    {collection, parsed}
  end

  defp parse_commit(_, _), do: {nil, :ignore}

  defp parse_commit_op(uri, did, collection, operation, commit)
       when collection in @indexed_collections and operation in ["create", "update"] do
    case commit do
      %{"record" => record} when is_map(record) ->
        cid = commit["cid"]

        attrs = %{
          uri: uri,
          collection: collection,
          author_did: did,
          cid: cid,
          record_jsonb: record,
          workspace_id: derive_workspace_id(uri, collection, record),
          supersedes_uri: record["supersedes"],
          is_workspace_root:
            collection == @directory_collection and record["isWorkspaceRoot"] == true
        }

        {:upsert_record, attrs}

      _ ->
        :ignore
    end
  end

  defp parse_commit_op(uri, _did, collection, "delete", _commit)
       when collection in @indexed_collections do
    {:delete_record, %{uri: uri}}
  end

  defp parse_commit_op(_uri, did, @account_config_collection, op, _commit)
       when op in ["create", "update"] do
    {:account_config_seen, %{did: did, op: op}}
  end

  defp parse_commit_op(_uri, did, @account_config_collection, "delete", _commit) do
    {:account_config_seen, %{did: did, op: "delete"}}
  end

  defp parse_commit_op(_uri, _did, _collection, _operation, _commit), do: :ignore

  # -- Workspace ID derivation ----------------------------------------
  #
  # Workspace identity is pulled from the record's own `workspaceId`
  # field for non-genesis records. The genesis keyring carries no
  # `workspaceId` (its own URI is the workspace ID) — we substitute its
  # own URI so downstream code never has to special-case this. Cabinet
  # records lack `workspaceId` entirely → nil.

  defp derive_workspace_id(_uri, @keyring_collection, %{"workspaceId" => ws}) when is_binary(ws),
    do: ws

  defp derive_workspace_id(_uri, @keyring_collection, %{"supersedes" => prior}) when is_binary(prior) do
    # Supersede keyring with no explicit workspaceId — try to resolve
    # via prior. Falls back to nil if the predecessor isn't indexed
    # yet; the dispatch handles `nil workspace_id` records as orphans.
    case OpakeIndexer.Queries.RecordQueries.lookup(prior) do
      %{workspace_id: ws} when is_binary(ws) -> ws
      _ -> nil
    end
  end

  defp derive_workspace_id(uri, @keyring_collection, _record), do: uri

  defp derive_workspace_id(_uri, _collection, %{"workspaceId" => ws}) when is_binary(ws), do: ws

  defp derive_workspace_id(_uri, _collection, _record), do: nil
end
