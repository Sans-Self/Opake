defmodule OpakeAppview.Auth.KeyCache do
  @moduledoc """
  In-memory cache for Ed25519 signing public keys, keyed by DID with a 5-minute
  TTL. Reads go directly to ETS (no bottleneck). Writes are serialized through
  the GenServer to prevent thundering-herd fetches for the same DID.
  """

  use GenServer
  require Logger

  @table :key_cache
  @ttl_ms :timer.minutes(5)

  # Public API — direct ETS reads, no bottleneck

  def start_link(_opts) do
    GenServer.start_link(__MODULE__, [], name: __MODULE__)
  end

  def get_key(did) do
    case ets_lookup(did) do
      {:ok, pubkey} -> {:ok, pubkey}
      _ -> GenServer.call(__MODULE__, {:fetch, did}, 15_000)
    end
  end

  # GenServer — serializes concurrent fetches for the same DID

  @impl true
  def init(_) do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    {:ok, %{}}
  end

  @impl true
  def handle_call({:fetch, did}, _from, state) do
    result =
      case ets_lookup(did) do
        {:ok, pubkey} ->
          {:ok, pubkey}

        _ ->
          case key_fetcher().fetch_signing_key(did) do
            {:ok, pubkey} ->
              now = System.monotonic_time(:millisecond)
              :ets.insert(@table, {did, pubkey, now})
              {:ok, pubkey}

            {:error, reason} ->
              {:error, reason}
          end
      end

    {:reply, result, state}
  end

  defp ets_lookup(did) do
    case :ets.lookup(@table, did) do
      [{^did, pubkey, inserted_at}] ->
        now = System.monotonic_time(:millisecond)

        if now - inserted_at < @ttl_ms do
          {:ok, pubkey}
        else
          :stale
        end

      [] ->
        :miss
    end
  end

  defp key_fetcher do
    Application.get_env(:opake_appview, :key_fetcher, OpakeAppview.Auth.KeyFetcher)
  end
end
