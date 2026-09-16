defmodule OpakeIndexer.Auth.KeyCache do
  @moduledoc """
  Caches a complete authentication-resolution decision for a short bounded
  lifetime. A DID document can add or remove `#opake` independently of the
  public-key record, so entries must expire as one unit with its history check.

  A refusal is cached like a success: a record that fails to verify keeps
  failing until it is rewritten, and caching the refusal is what stops a bad
  credential from hammering the PDS. A transport failure is not cached at all,
  so a PLC or PDS outage stops returning `503` the moment it recovers rather
  than for a further lifetime.

  Expired entries are reclaimed by a sweep amortised over inserts, not on every
  miss. Capacity eviction drops the oldest entry.

  The clock is injectable (`:key_cache_clock`, a zero-arity function returning
  milliseconds) so expiry can be exercised without sleeping.
  """

  use GenServer
  require Logger

  @table :key_cache
  @counters :key_cache_counters
  @sweep_every 256

  def start_link(_opts), do: GenServer.start_link(__MODULE__, [], name: __MODULE__)

  @spec get_key(String.t()) :: {:ok, binary()} | {:error, term()}
  def get_key(did) do
    with {:ok, %{key: key}} <- get_decision(did), do: {:ok, key}
  end

  @spec get_decision(String.t()) ::
          {:ok, OpakeIndexer.Auth.KeyFetcherBehaviour.decision()} | {:error, term()}
  def get_decision(did) do
    now = now_ms()

    case :ets.lookup(@table, did) do
      [{^did, _cached_at, expires_at, decision}] when expires_at > now ->
        decision

      _ ->
        decision = fetch_decision(did)
        cache(did, decision, now)
        decision
    end
  end

  defp fetch_decision(did) do
    decision = key_fetcher().fetch_authentication_decision(did)
    log_anchor_history(did, decision)
    decision
  end

  # The indexer has no response channel for a resolution notice, so a replaced
  # anchor surfaces to operators in the log. Firing on the fetch path rate-limits
  # it to once per DID per cache lifetime.
  defp log_anchor_history(did, {:ok, %{anchor_history: :replaced}}) do
    Logger.warning(
      "#{did} authenticated against a replaced #opake verification method: the DID document's " <>
        "verification method has been replaced with a different key since the account anchored"
    )
  end

  defp log_anchor_history(did, {:ok, %{anchor_history: :unavailable}}) do
    Logger.info(
      "#{did} authenticated without a readable anchor history; replacement status is unknown"
    )
  end

  defp log_anchor_history(_did, _decision), do: :ok

  @impl true
  def init(_) do
    :ets.new(@table, [
      :named_table,
      :set,
      :public,
      read_concurrency: true,
      write_concurrency: true
    ])

    :ets.new(@counters, [:named_table, :set, :public, write_concurrency: true])
    :ets.insert(@counters, {:inserts, 0})

    {:ok, nil}
  end

  defp cache(_did, {:error, {:unavailable, _source}}, _now), do: :ok

  defp cache(did, decision, now) do
    if over_capacity?() or sweep_due?(), do: sweep_expired(now)
    if over_capacity?(), do: evict_oldest()
    :ets.insert(@table, {did, now, now + ttl_ms(), decision})
    :ok
  end

  defp over_capacity?, do: :ets.info(@table, :size) >= max_entries()

  defp sweep_due? do
    rem(:ets.update_counter(@counters, :inserts, {2, 1, @sweep_every, 1}), @sweep_every) == 0
  end

  defp sweep_expired(now) do
    :ets.select_delete(@table, [{{:_, :_, :"$1", :_}, [{:"=<", :"$1", now}], [true]}])
  end

  defp evict_oldest do
    case :ets.select(@table, [{{:"$1", :"$2", :_, :_}, [], [{{:"$2", :"$1"}}]}]) do
      [] -> :ok
      entries -> entries |> Enum.min() |> elem(1) |> then(&:ets.delete(@table, &1))
    end
  end

  defp now_ms do
    case Application.get_env(:opake_indexer, :key_cache_clock) do
      nil -> System.monotonic_time(:millisecond)
      clock when is_function(clock, 0) -> clock.()
    end
  end

  defp ttl_ms do
    Application.get_env(:opake_indexer, :key_cache_ttl_ms, 300_000)
  end

  defp max_entries do
    Application.get_env(:opake_indexer, :key_cache_max_entries, 10_000)
  end

  defp key_fetcher do
    Application.get_env(:opake_indexer, :key_fetcher, OpakeIndexer.Auth.KeyFetcher)
  end
end
