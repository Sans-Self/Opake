defmodule OpakeIndexer.Backfill do
  @moduledoc """
  Backfill records from a PDS when the firehose cursor is absent or stale.

  Fetches all `app.opake.*` collections via `com.atproto.repo.listRecords`
  (public, unauthenticated endpoint) and upserts them through the same query
  path as the indexer.

  ## Supported collections

  - `app.opake.keyring` — workspace records + member lists (critical for
    membership checks — missing keyrings cause 403s on workspace endpoints)
  - `app.opake.directory` — directory tree structure + encrypted metadata
  - `app.opake.document` — document records + encryption metadata + blob refs
  - `app.opake.grant` — sharing grants (inbox visibility)

  ## Usage

  - `Backfill.backfill_did(did)` — backfill all collections for a single DID
  - `Backfill.backfill_known_dids()` — backfill every DID found in `keyring_members`
  - `mix opake.resync <did>` — CLI trigger for dev/ops

  ## Not backfilled

  `directoryUpdate`, `keyringUpdate`, `documentUpdate` — these are transient
  proposal records consumed by workspace owners. Not needed for tree
  reconstruction (the applied results are already in the base records).
  """

  require Logger

  alias OpakeIndexer.Queries.{DirectoryQueries, DocumentQueries, GrantQueries, KeyringQueries}

  @keyring_collection "app.opake.keyring"
  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @grant_collection "app.opake.grant"

  @collections [
    @keyring_collection,
    @directory_collection,
    @document_collection,
    @grant_collection
  ]

  @spec backfill_did(String.t()) :: :ok | {:error, term()}
  def backfill_did(did) do
    with {:ok, pds_url} <- resolve_pds(did) do
      Logger.info("[Backfill] #{did} from #{pds_url}")
      now = DateTime.utc_now()

      results =
        Enum.map(@collections, fn collection ->
          case backfill_collection(did, pds_url, collection, now) do
            {:ok, count} ->
              Logger.info("[Backfill] #{collection}: #{count} record(s)")
              {:ok, collection, count}

            {:error, reason} ->
              Logger.warning("[Backfill] #{collection} failed: #{inspect(reason)}")
              {:error, collection, reason}
          end
        end)

      errors = Enum.filter(results, &match?({:error, _, _}, &1))

      if errors == [] do
        total = results |> Enum.map(fn {:ok, _, n} -> n end) |> Enum.sum()
        Logger.info("[Backfill] complete for #{did}: #{total} total record(s)")
        :ok
      else
        Logger.warning("[Backfill] partial failure for #{did}: #{length(errors)} collection(s)")
        {:error, :partial_failure}
      end
    end
  end

  @spec backfill_known_dids() :: :ok
  def backfill_known_dids do
    dids = KeyringQueries.all_member_dids()
    Logger.info("[Backfill] #{length(dids)} known DID(s)")

    Enum.each(dids, fn did ->
      case backfill_did(did) do
        :ok -> :ok
        {:error, reason} -> Logger.warning("[Backfill] failed for #{did}: #{inspect(reason)}")
      end
    end)
  end

  # -- Per-collection backfill --

  defp backfill_collection(did, pds_url, collection, now) do
    case list_records(pds_url, did, collection) do
      {:ok, records} ->
        Enum.each(records, fn record ->
          index_record(did, collection, record, now)
        end)

        {:ok, length(records)}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # -- Record indexing (reuses the same query paths as the firehose indexer) --

  @spec index_record(String.t(), String.t(), map(), DateTime.t()) ::
          :ok | {:ok, term()} | {:error, term()}

  def index_record(did, @keyring_collection, %{"uri" => uri, "value" => record}, _now) do
    members = record["members"] || []

    member_entries =
      members
      |> Enum.filter(&is_map/1)
      |> Enum.map(fn m ->
        wrapped = m["wrappedKey"] || %{}
        %{did: wrapped["did"], role: m["role"], wrapped_key: wrapped}
      end)
      |> Enum.filter(fn entry -> is_binary(entry.did) end)

    KeyringQueries.upsert_keyring(uri, did, member_entries)

    KeyringQueries.upsert_keyring_record(%{
      uri: uri,
      owner_did: did,
      rotation: record["rotation"],
      encrypted_metadata: record["encryptedMetadata"],
      created_at: record["createdAt"]
    })
  end

  def index_record(did, @directory_collection, %{"uri" => uri, "value" => record}, now) do
    key_wrapping = record["keyWrapping"]
    encrypted_metadata = record["encryptedMetadata"]

    entries =
      case record["entries"] do
        list when is_list(list) -> Enum.filter(list, &is_binary/1)
        _ -> []
      end

    keyring_uri =
      case key_wrapping do
        %{"keyringRef" => %{"keyring" => kr}} when is_binary(kr) -> kr
        _ -> nil
      end

    DirectoryQueries.upsert_directory(%{
      directory_uri: uri,
      keyring_uri: keyring_uri,
      owner_did: did,
      entries: entries,
      encrypted_metadata: encrypted_metadata,
      key_wrapping: key_wrapping,
      deleted_at: nil,
      indexed_at: now
    })
  end

  def index_record(did, @document_collection, %{"uri" => uri, "value" => record}, now) do
    encryption = record["encryption"]
    encrypted_metadata = record["encryptedMetadata"]
    blob = record["blob"]

    {keyring_uri, rotation} =
      case encryption do
        %{"keyringRef" => %{"keyring" => kr, "rotation" => rot}}
        when is_binary(kr) and is_integer(rot) ->
          {kr, rot}

        _ ->
          {nil, nil}
      end

    DocumentQueries.upsert_document(%{
      document_uri: uri,
      keyring_uri: keyring_uri,
      owner_did: did,
      rotation: rotation,
      encrypted_metadata: encrypted_metadata,
      encryption: encryption,
      blob_ref: blob,
      deleted_at: nil,
      indexed_at: now
    })
  end

  def index_record(did, @grant_collection, %{"uri" => uri, "value" => record}, now) do
    recipient = record["recipient"]
    document = record["document"]
    created_at = record["createdAt"]

    if is_binary(recipient) and is_binary(document) and is_binary(created_at) do
      GrantQueries.upsert_grant(%{
        uri: uri,
        owner_did: did,
        recipient_did: recipient,
        document_uri: document,
        created_at: created_at,
        indexed_at: now
      })
    else
      :ok
    end
  end

  def index_record(_, _, _, _), do: :ok

  # -- PDS resolution --

  @spec resolve_pds(String.t()) :: {:ok, String.t()} | {:error, term()}
  defp resolve_pds(did) do
    url = "https://plc.directory/#{did}"

    case Req.get(url) do
      {:ok, %{status: 200, body: body}} ->
        doc = if is_binary(body), do: Jason.decode!(body), else: body

        case doc["service"] do
          services when is_list(services) ->
            case Enum.find(services, &(&1["id"] == "#atproto_pds")) do
              %{"serviceEndpoint" => endpoint} -> {:ok, endpoint}
              _ -> {:error, :no_pds_service}
            end

          _ ->
            {:error, :invalid_did_doc}
        end

      {:ok, %{status: status}} ->
        {:error, {:plc_status, status}}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # -- Paginated record listing --

  @spec list_records(String.t(), String.t(), String.t()) :: {:ok, [map()]} | {:error, term()}
  defp list_records(pds_url, did, collection) do
    list_records_paginated(pds_url, did, collection, nil, [])
  end

  defp list_records_paginated(pds_url, did, collection, cursor, acc) do
    params = %{repo: did, collection: collection, limit: 100}
    params = if cursor, do: Map.put(params, :cursor, cursor), else: params
    url = "#{pds_url}/xrpc/com.atproto.repo.listRecords"

    case Req.get(url, params: params) do
      {:ok, %{status: 200, body: %{"records" => records} = body}} ->
        new_acc = acc ++ records

        case body["cursor"] do
          nil -> {:ok, new_acc}
          next_cursor -> list_records_paginated(pds_url, did, collection, next_cursor, new_acc)
        end

      {:ok, %{status: status, body: body}} ->
        {:error, {:pds_error, status, body}}

      {:error, reason} ->
        {:error, reason}
    end
  end
end
