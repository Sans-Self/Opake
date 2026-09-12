defmodule OpakeIndexer.Auth.KeyCache do
  @moduledoc """
  Resolves a fresh authentication decision for every request. A DID document can
  add or remove `#opake` independently of the public-key record, so caching a
  previous successful decision would allow a stale verification state.
  """

  use GenServer
  require Logger

  @table :key_cache
  # Public API — direct ETS reads, no bottleneck

  def start_link(_opts) do
    GenServer.start_link(__MODULE__, [], name: __MODULE__)
  end

  def get_key(did) do
    GenServer.call(__MODULE__, {:fetch, did}, 15_000)
  end

  # GenServer — serializes concurrent fetches for the same DID

  @impl true
  def init(_) do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    {:ok, %{}}
  end

  @impl true
  def handle_call({:fetch, did}, _from, state) do
    result = key_fetcher().fetch_signing_key(did)

    {:reply, result, state}
  end

  defp key_fetcher do
    Application.get_env(:opake_indexer, :key_fetcher, OpakeIndexer.Auth.KeyFetcher)
  end
end
