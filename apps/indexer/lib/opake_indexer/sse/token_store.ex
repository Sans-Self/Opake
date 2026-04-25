defmodule OpakeIndexer.SSE.TokenStore do
  @moduledoc """
  ETS-backed single-use token store for SSE authentication.

  Tokens are short-lived (60s TTL) and consumed on first use. This
  sidesteps EventSource's inability to send custom headers — the client
  POSTs with Ed25519 auth to get a token, then passes it as a query
  parameter to the SSE endpoint.
  """

  @table :sse_tokens
  @ttl_ms 60_000
  @cleanup_interval_ms 60_000

  use GenServer

  # -- Public API --

  @spec init_table() :: :ok
  def init_table do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    :ok
  end

  def start_link(_opts) do
    GenServer.start_link(__MODULE__, [], name: __MODULE__)
  end

  @spec create_token(String.t()) :: String.t()
  def create_token(did) when is_binary(did) do
    token =
      :crypto.strong_rand_bytes(32)
      |> Base.url_encode64(padding: false)

    expires_at = System.monotonic_time(:millisecond) + @ttl_ms
    :ets.insert(@table, {token, did, expires_at})
    token
  end

  @spec consume_token(String.t()) :: {:ok, String.t()} | :error
  def consume_token(token) when is_binary(token) do
    case :ets.take(@table, token) do
      [{^token, did, expires_at}] ->
        if System.monotonic_time(:millisecond) <= expires_at do
          {:ok, did}
        else
          :error
        end

      [] ->
        :error
    end
  end

  @spec ttl_seconds() :: integer()
  def ttl_seconds, do: div(@ttl_ms, 1000)

  # -- GenServer (periodic cleanup) --

  @impl true
  def init(_) do
    schedule_cleanup()
    {:ok, %{}}
  end

  @impl true
  def handle_info(:cleanup, state) do
    cleanup_expired()
    schedule_cleanup()
    {:noreply, state}
  end

  defp schedule_cleanup do
    Process.send_after(self(), :cleanup, @cleanup_interval_ms)
  end

  defp cleanup_expired do
    now = System.monotonic_time(:millisecond)

    :ets.select_delete(@table, [
      {{:_, :_, :"$1"}, [{:<, :"$1", now}], [true]}
    ])
  end
end
