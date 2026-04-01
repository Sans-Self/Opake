defmodule OpakeAppview.Backfill do
  @moduledoc """
  Backfill records from a PDS when the firehose cursor is absent or stale.
  Fetches directory and document records via com.atproto.repo.listRecords
  and upserts them through the same query path as the indexer.
  """

  require Logger

  alias OpakeAppview.Queries.{DirectoryQueries, DocumentQueries, KeyringQueries}

  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"

  @spec backfill_did(String.t()) :: :ok | {:error, term()}
  def backfill_did(did) do
    with {:ok, pds_url} <- resolve_pds(did) do
      Logger.info("Backfilling #{did} from #{pds_url}")
      now = DateTime.utc_now()

      with :ok <- backfill_collection(did, pds_url, @directory_collection, now),
           :ok <- backfill_collection(did, pds_url, @document_collection, now) do
        Logger.info("Backfill complete for #{did}")
        :ok
      end
    end
  end

  @spec backfill_known_dids() :: :ok
  def backfill_known_dids do
    dids = KeyringQueries.all_member_dids()
    Logger.info("Backfilling #{length(dids)} known DIDs")

    Enum.each(dids, fn did ->
      case backfill_did(did) do
        :ok -> :ok
        {:error, reason} -> Logger.warning("Backfill failed for #{did}: #{inspect(reason)}")
      end
    end)
  end

  defp backfill_collection(did, pds_url, collection, now) do
    case list_records(pds_url, did, collection) do
      {:ok, records} ->
        Enum.each(records, fn record ->
          index_record(did, collection, record, now)
        end)

        :ok

      {:error, reason} ->
        Logger.warning("Failed to list #{collection} for #{did}: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @spec index_record(String.t(), String.t(), map(), DateTime.t()) ::
          :ok | {:ok, term()} | {:error, term()}
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

  def index_record(_, _, _, _), do: :ok

  @spec resolve_pds(String.t()) :: {:ok, String.t()} | {:error, term()}
  defp resolve_pds(did) do
    url = "https://plc.directory/#{did}"

    case Req.get(url) do
      {:ok, %{status: 200, body: body}} ->
        case get_in(body, ["service"]) do
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
