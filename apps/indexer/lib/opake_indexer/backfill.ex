defmodule OpakeIndexer.Backfill do
  @moduledoc """
  Backfill records from a PDS when the firehose cursor is absent or stale.

  Fetches all `app.opake.*` collections via `com.atproto.repo.listRecords`
  (public, unauthenticated endpoint) and feeds each record through the
  firehose dispatch as a synthetic create commit. Routing the backfill
  path through the same code as the live indexer guarantees parse logic,
  chain dispatch, authority validation, and SSE broadcasting are
  identical — no second source of truth.

  ## Supported collections

  - `app.opake.keyring` — workspace records (chain members + heads)
  - `app.opake.directory` — directory tree records
  - `app.opake.document` — document records (workspace + cabinet)
  - `app.opake.grant` — sharing grants

  ## Usage

  - `Backfill.backfill_did(did)` — backfill all collections for a single DID
  - `Backfill.backfill_known_dids()` — backfill every DID found in `keyring_members`
  - `mix opake.resync <did>` — CLI trigger for dev/ops
  """

  require Logger

  import Ecto.Query

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Record, as: RecordSchema

  @keyring_collection "app.opake.keyring"
  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @grant_collection "app.opake.grant"

  # Ordering matters during a single-DID backfill: keyrings carry workspace
  # identity that directories and documents reference. Indexing keyrings
  # first means the chain dispatch can resolve workspace_id by joining
  # through the keyring rows rather than rejecting orphans.
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

      results =
        Enum.map(@collections, fn collection ->
          case backfill_collection(did, pds_url, collection) do
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
    dids = all_known_dids()
    Logger.info("[Backfill] #{length(dids)} known DID(s)")

    Enum.each(dids, fn did ->
      case backfill_did(did) do
        :ok -> :ok
        {:error, reason} -> Logger.warning("[Backfill] failed for #{did}: #{inspect(reason)}")
      end
    end)
  end

  # Pulls every DID we've ever indexed a record for. Useful for ops-level
  # "rebuild from PDSes" flows; not on any hot path.
  defp all_known_dids do
    Repo.all(from(r in RecordSchema, select: r.author_did, distinct: true))
  end

  # -- Per-collection backfill --

  defp backfill_collection(did, pds_url, collection) do
    case list_records(pds_url, did, collection) do
      {:ok, records} ->
        Enum.each(records, fn record ->
          dispatch_via_firehose(did, collection, record)
        end)

        {:ok, length(records)}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # -- Firehose dispatch (single source of truth) --

  # Construct a synthetic Jetstream-shaped commit JSON and feed it through
  # the firehose. The firehose's parser handles the new record shapes and
  # its dispatch runs chain logic, authority validation, and broadcasting.
  defp dispatch_via_firehose(did, collection, %{"uri" => uri, "value" => record} = entry) do
    rkey = extract_rkey(uri)
    cid = entry["cid"]

    synthetic =
      %{
        "kind" => "commit",
        "did" => did,
        # No real time_us during backfill — cursor save throttles on
        # nil, so this won't corrupt the live cursor.
        "time_us" => nil,
        "commit" => %{
          "operation" => "create",
          "collection" => collection,
          "rkey" => rkey,
          "cid" => cid,
          "record" => record
        }
      }
      |> Jason.encode!()

    Firehose.process_message(synthetic, 0)
    :ok
  end

  defp dispatch_via_firehose(_, _, _), do: :ok

  defp extract_rkey(uri) do
    uri
    |> String.split("/")
    |> List.last()
  end

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
