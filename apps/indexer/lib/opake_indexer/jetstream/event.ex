defmodule OpakeIndexer.Jetstream.Event do
  @moduledoc """
  Parses raw Jetstream JSON messages into tagged tuples for the indexer.

  ## Return shape

  `parse/1` always returns `{time_us, collection, payload}` where:

    * `time_us` — the event's `time_us` field if present, else `nil`
    * `collection` — the commit's collection string if present, else `nil`
      (set even for collections we ignore, so the indexer can bump
      per-collection counters without re-decoding)
    * `payload` — one of the tagged event tuples below, or `:ignore`

  Returning these three pieces from a single decode means the indexer
  never has to re-parse the JSON to advance its cursor or update its
  per-collection counters.

  ## Recognized collections

  Seven `app.opake.*` collections (grant, keyring, document, directory,
  documentUpdate, directoryUpdate, keyringUpdate). Everything else
  (identity events, bsky lexicons, malformed JSON) returns `:ignore`.
  """

  @grant_collection "app.opake.grant"
  @keyring_collection "app.opake.keyring"
  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @document_update_collection "app.opake.documentUpdate"
  @directory_update_collection "app.opake.directoryUpdate"
  @keyring_update_collection "app.opake.keyringUpdate"
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
          | {:upsert_document_update, map()}
          | {:delete_document_update, map()}
          | {:upsert_directory_update, map()}
          | {:delete_directory_update, map()}
          | {:upsert_keyring_update, map()}
          | {:delete_keyring_update, map()}
          | {:account_config_seen, map()}
          | :ignore

  @type result ::
          {time_us :: integer() | nil, collection :: String.t() | nil, event_payload()}

  @doc """
  Parses a Jetstream WebSocket frame.

  Always decodes the JSON exactly once and returns
  `{time_us, collection, payload}`. Time_us and collection may both be
  `nil` for malformed JSON or non-commit events.
  """
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

    parsed = parse_commit_op(uri, did, collection, operation, commit)
    {collection, parsed}
  end

  defp parse_commit(_, _), do: {nil, :ignore}

  defp parse_commit_op(uri, did, collection, operation, commit) do
    case {collection, operation} do
      {@grant_collection, op} when op in ["create", "update"] ->
        parse_grant_upsert(uri, did, commit)

      {@grant_collection, "delete"} ->
        {:delete_grant, %{uri: uri}}

      {@keyring_collection, op} when op in ["create", "update"] ->
        parse_keyring_upsert(uri, did, commit)

      {@keyring_collection, "delete"} ->
        {:delete_keyring, %{uri: uri}}

      {@directory_collection, op} when op in ["create", "update"] ->
        parse_directory_upsert(uri, did, commit)

      {@directory_collection, "delete"} ->
        {:delete_directory, %{directory_uri: uri}}

      {@document_collection, op} when op in ["create", "update"] ->
        parse_document_upsert(uri, did, commit)

      {@document_collection, "delete"} ->
        {:delete_document, %{document_uri: uri}}

      {@document_update_collection, op} when op in ["create", "update"] ->
        parse_document_update_upsert(uri, did, commit)

      {@document_update_collection, "delete"} ->
        {:delete_document_update, %{uri: uri}}

      {@directory_update_collection, op} when op in ["create", "update"] ->
        parse_directory_update_upsert(uri, did, commit)

      {@directory_update_collection, "delete"} ->
        {:delete_directory_update, %{uri: uri}}

      {@keyring_update_collection, op} when op in ["create", "update"] ->
        parse_keyring_update_upsert(uri, did, commit)

      {@keyring_update_collection, "delete"} ->
        {:delete_keyring_update, %{uri: uri}}

      # Heartbeat signal: web app writes accountConfig periodically. Logged
      # only — not persisted to the indexer DB. Provides a regular proof-of-life
      # for the indexer when no real Opake activity is happening.
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
         owner_did: did,
         recipient_did: recipient,
         document_uri: document,
         created_at: created_at
       }}
    else
      :ignore
    end
  end

  defp parse_grant_upsert(_, _, _), do: :ignore

  defp parse_keyring_upsert(uri, did, %{"record" => record}) when is_map(record) do
    members = record["members"] || []

    member_entries =
      members
      |> Enum.filter(&is_map/1)
      |> Enum.map(fn m ->
        wrapped = m["wrappedKey"] || %{}
        %{did: wrapped["did"], role: m["role"], wrapped_key: wrapped}
      end)
      |> Enum.filter(fn entry -> is_binary(entry.did) end)

    {:upsert_keyring,
     %{
       uri: uri,
       owner_did: did,
       member_entries: member_entries,
       rotation: record["rotation"],
       encrypted_metadata: record["encryptedMetadata"],
       created_at: record["createdAt"]
     }}
  end

  defp parse_keyring_upsert(_, _, _), do: :ignore

  defp parse_directory_upsert(uri, did, %{"record" => record}) when is_map(record) do
    key_wrapping = record["keyWrapping"]
    encrypted_metadata = record["encryptedMetadata"]

    entries =
      case record["entries"] do
        list when is_list(list) -> Enum.filter(list, &is_binary/1)
        _ -> []
      end

    keyring_uri =
      case key_wrapping do
        %{"keyringRef" => %{"keyring" => kr_uri}} when is_binary(kr_uri) -> kr_uri
        _ -> nil
      end

    {:upsert_directory,
     %{
       directory_uri: uri,
       keyring_uri: keyring_uri,
       owner_did: did,
       entries: entries,
       encrypted_metadata: encrypted_metadata,
       key_wrapping: key_wrapping
     }}
  end

  defp parse_directory_upsert(_, _, _), do: :ignore

  defp parse_document_upsert(uri, did, %{"record" => record}) when is_map(record) do
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
       document_uri: uri,
       keyring_uri: keyring_uri,
       owner_did: did,
       rotation: rotation,
       encrypted_metadata: encrypted_metadata,
       encryption: encryption,
       blob_ref: blob
     }}
  end

  defp parse_document_upsert(_, _, _), do: :ignore

  defp parse_document_update_upsert(uri, did, %{"record" => record})
       when is_map(record) do
    document = record["document"]
    supersedes = record["supersedes"]

    if is_binary(document) do
      {:upsert_document_update,
       %{
         uri: uri,
         author_did: did,
         document_uri: document,
         supersedes_uri: supersedes
       }}
    else
      :ignore
    end
  end

  defp parse_document_update_upsert(_, _, _), do: :ignore

  defp parse_directory_update_upsert(uri, did, %{"record" => record})
       when is_map(record) do
    keyring = record["keyring"]
    action_type = record["actionType"]

    valid_fields =
      is_binary(keyring) and is_binary(action_type) and
        valid_directory_update_fields?(action_type, record)

    if not valid_fields and is_binary(action_type) do
      require Logger
      Logger.error("Rejected directoryUpdate #{uri}: #{action_type} missing required camelCase fields")
    end

    if valid_fields do
      {:upsert_directory_update,
       %{
         uri: uri,
         keyring_uri: keyring,
         author_did: did,
         action_type: action_type,
         directory_uri: record["directory"],
         entry_uri: record["entry"],
         encrypted_metadata: record["encryptedMetadata"],
         source_directory_uri: record["sourceDirectory"],
         target_directory_uri: record["targetDirectory"],
         parent_directory_uri: record["parentDirectory"]
       }}
    else
      :ignore
    end
  end

  defp parse_directory_update_upsert(_, _, _), do: :ignore

  # Validate that action-specific required fields are present (camelCase wire format).
  # Rejects records with snake_case fields from old WASM builds.
  defp valid_directory_update_fields?("addEntry", r),
    do: is_binary(r["directory"]) and is_binary(r["entry"])

  defp valid_directory_update_fields?("removeEntry", r),
    do: is_binary(r["directory"]) and is_binary(r["entry"])

  defp valid_directory_update_fields?("moveEntry", r),
    do: is_binary(r["sourceDirectory"]) and is_binary(r["targetDirectory"]) and is_binary(r["entry"])

  defp valid_directory_update_fields?("createDirectory", r),
    do: is_binary(r["parentDirectory"]) and is_map(r["encryptedMetadata"])

  defp valid_directory_update_fields?("deleteDirectory", r),
    do: is_binary(r["directory"])

  defp valid_directory_update_fields?("renameDirectory", r),
    do: is_binary(r["directory"]) and is_map(r["encryptedMetadata"])

  defp valid_directory_update_fields?(_, _), do: true

  defp parse_keyring_update_upsert(uri, did, %{"record" => record})
       when is_map(record) do
    keyring = record["keyring"]
    action_type = record["actionType"]

    if is_binary(keyring) and is_binary(action_type) do
      member_public_key =
        case record["memberPublicKey"] do
          %{"$bytes" => b64} when is_binary(b64) -> Base.decode64!(b64)
          _ -> nil
        end

      {:upsert_keyring_update,
       %{
         uri: uri,
         keyring_uri: keyring,
         author_did: did,
         action_type: action_type,
         member_did: record["memberDid"],
         member_public_key: member_public_key,
         role: record["role"],
         encrypted_metadata: record["encryptedMetadata"]
       }}
    else
      :ignore
    end
  end

  defp parse_keyring_update_upsert(_, _, _), do: :ignore
end
