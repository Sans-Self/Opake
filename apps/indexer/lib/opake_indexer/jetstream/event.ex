defmodule OpakeIndexer.Jetstream.Event do
  @moduledoc """
  Parses raw Jetstream JSON messages into tagged tuples for the indexer.

  `parse/1` always returns `{time_us, collection, payload}` where:

    * `time_us` — the event's `time_us` field if present, else `nil`
    * `collection` — the commit's collection string if present, else `nil`
      (set even for collections we ignore, so the indexer can bump
      per-collection counters without re-decoding)
    * `payload` — one of the tagged event tuples below, or `:ignore`

  ## Recognized collections

  Five `app.opake.*` collections (grant, keyring, document, directory,
  accountConfig). The proposal-era *Update collections are gone with the
  federation rewrite — keyring/directory mutations are curatorial
  supersedes on the same record types.
  """

  @grant_collection "app.opake.grant"
  @keyring_collection "app.opake.keyring"
  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @account_config_collection "app.opake.accountConfig"

  @type event_payload ::
          {:upsert_grant, map()}
          | {:delete_grant, map()}
          | {:upsert_keyring, map()}
          | {:delete_keyring, map()}
          | {:upsert_directory, map()}
          | {:delete_directory, map()}
          | {:upsert_document, map()}
          | {:delete_document, map()}
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

  # -- Internal --

  defp extract_time_us(%{"time_us" => t}) when is_integer(t), do: t
  defp extract_time_us(_), do: nil

  defp parse_payload(%{
         "kind" => "commit",
         "did" => did,
         "commit" => commit
       }) do
    parse_commit(did, commit)
  end

  defp parse_payload(_), do: {nil, :ignore}

  defp parse_commit(
         did,
         %{
           "operation" => operation,
           "collection" => collection,
           "rkey" => rkey
         } = commit
       )
       when is_binary(collection) do
    uri = "at://#{did}/#{collection}/#{rkey}"
    cid = commit["cid"]

    parsed = parse_commit_op(uri, cid, did, collection, operation, commit)
    {collection, parsed}
  end

  defp parse_commit(_, _), do: {nil, :ignore}

  defp parse_commit_op(uri, cid, did, collection, operation, commit) do
    case {collection, operation} do
      {@grant_collection, op} when op in ["create", "update"] ->
        parse_grant_upsert(uri, did, commit)

      {@grant_collection, "delete"} ->
        {:delete_grant, %{uri: uri}}

      {@keyring_collection, op} when op in ["create", "update"] ->
        parse_keyring_upsert(uri, cid, did, commit)

      {@keyring_collection, "delete"} ->
        {:delete_keyring, %{uri: uri}}

      {@directory_collection, op} when op in ["create", "update"] ->
        parse_directory_upsert(uri, cid, did, commit)

      {@directory_collection, "delete"} ->
        {:delete_directory, %{uri: uri}}

      {@document_collection, op} when op in ["create", "update"] ->
        parse_document_upsert(uri, cid, did, commit)

      {@document_collection, "delete"} ->
        {:delete_document, %{uri: uri}}

      # Heartbeat signal: web app writes accountConfig periodically. Logged
      # only — not persisted to the indexer DB.
      {@account_config_collection, op} when op in ["create", "update"] ->
        {:account_config_seen, %{did: did, op: op}}

      {@account_config_collection, "delete"} ->
        {:account_config_seen, %{did: did, op: "delete"}}

      _ ->
        :ignore
    end
  end

  defp parse_grant_upsert(uri, did, %{"record" => record}) when is_map(record) do
    recipient = record["recipient"]
    document = record["document"]
    created_at = record["createdAt"]

    if is_binary(recipient) and is_binary(document) and is_binary(created_at) do
      {:upsert_grant,
       %{
         uri: uri,
         author_did: did,
         recipient_did: recipient,
         document_uri: document,
         created_at: created_at
       }}
    else
      :ignore
    end
  end

  defp parse_grant_upsert(_, _, _), do: :ignore

  defp parse_keyring_upsert(uri, cid, did, %{"record" => record}) when is_map(record) do
    members = record["members"] || []

    member_entries =
      members
      |> Enum.filter(&is_map/1)
      |> Enum.map(fn m ->
        wrapped = m["wrappedKey"] || %{}
        %{did: wrapped["did"], role: m["role"], wrapped_key: wrapped}
      end)
      |> Enum.filter(fn entry -> is_binary(entry.did) end)

    # workspace_id is absent on genesis (the URI itself is the workspace ID).
    # Supersedes carry an explicit workspaceId pointing back at the genesis.
    workspace_id = record["workspaceId"] || if is_nil(record["supersedes"]), do: uri, else: nil

    {:upsert_keyring,
     %{
       uri: uri,
       cid: cid,
       author_did: did,
       workspace_id: workspace_id,
       member_entries: member_entries,
       rotation: record["rotation"],
       encrypted_metadata: record["encryptedMetadata"],
       supersedes_uri: record["supersedes"],
       created_at: record["createdAt"],
       modified_at: record["modifiedAt"]
     }}
  end

  defp parse_keyring_upsert(_, _, _, _), do: :ignore

  defp parse_directory_upsert(uri, cid, did, %{"record" => record}) when is_map(record) do
    key_wrapping = record["keyWrapping"]
    encrypted_metadata = record["encryptedMetadata"]

    # New entries shape: [{target: at-uri, targetCid: cid-string}]. Strict — a
    # malformed entry rejects the whole record so we don't end up with partial
    # listings.
    entries =
      case record["entries"] do
        list when is_list(list) ->
          parsed =
            Enum.map(list, fn
              %{"target" => target, "targetCid" => cid}
              when is_binary(target) and is_binary(cid) ->
                %{"target" => target, "target_cid" => cid}

              _ ->
                :invalid
            end)

          if Enum.any?(parsed, &(&1 == :invalid)), do: :invalid, else: parsed

        _ ->
          :invalid
      end

    keyring_uri =
      case key_wrapping do
        %{"keyringRef" => %{"keyring" => kr_uri}} when is_binary(kr_uri) -> kr_uri
        _ -> nil
      end

    if entries == :invalid do
      :ignore
    else
      {:upsert_directory,
       %{
         uri: uri,
         cid: cid,
         author_did: did,
         workspace_id: record["workspaceId"],
         keyring_uri: keyring_uri,
         entries_json: entries,
         encrypted_metadata: encrypted_metadata,
         key_wrapping: key_wrapping,
         supersedes_uri: record["supersedes"],
         modified_at: record["modifiedAt"]
       }}
    end
  end

  defp parse_directory_upsert(_, _, _, _), do: :ignore

  defp parse_document_upsert(uri, cid, did, %{"record" => record}) when is_map(record) do
    encryption = record["encryption"]
    encrypted_metadata = record["encryptedMetadata"]
    blob = record["blob"]

    {keyring_uri, rotation} =
      case encryption do
        %{"keyringRef" => %{"keyring" => kr_uri, "rotation" => rot}}
        when is_binary(kr_uri) and is_integer(rot) ->
          {kr_uri, rot}

        _ ->
          {nil, nil}
      end

    {:upsert_document,
     %{
       uri: uri,
       cid: cid,
       author_did: did,
       workspace_id: record["workspaceId"],
       keyring_uri: keyring_uri,
       rotation: rotation,
       encrypted_metadata: encrypted_metadata,
       encryption: encryption,
       blob_ref: blob,
       supersedes_uri: record["supersedes"],
       modified_at: record["modifiedAt"]
     }}
  end

  defp parse_document_upsert(_, _, _, _), do: :ignore
end
