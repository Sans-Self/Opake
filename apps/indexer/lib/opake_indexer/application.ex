defmodule OpakeIndexer.Application do
  @moduledoc """
  OTP application for the Opake Indexer — a read-only Jetstream indexer and
  query API for encrypted sharing metadata on the AT Protocol.

  Supervision tree:
    - Repo (Ecto/Postgres connection pool)
    - Auth.KeyCache (GenServer + ETS for Ed25519 public key caching)
    - Endpoint (Phoenix/Bandit HTTP server — always started, `server: false` skips binding)
    - Jetstream.Consumer (WebSockex — conditional on `:indexer_enabled` config)
  """

  use Application

  @impl true
  def start(_type, _args) do
    OpakeIndexer.Firehose.init_state()
    OpakeIndexer.Jetstream.Compression.init()
    OpakeIndexer.SSE.TokenStore.init_table()
    OpakeIndexer.SSE.ConnectionTracker.init_table()

    children =
      [
        OpakeIndexer.Repo,
        {Phoenix.PubSub, name: OpakeIndexer.PubSub},
        OpakeIndexer.SSE.TokenStore,
        OpakeIndexer.Auth.KeyCache,
        OpakeIndexerWeb.Endpoint
      ] ++
        maybe_indexer_children()

    opts = [strategy: :one_for_one, name: OpakeIndexer.Supervisor]
    Supervisor.start_link(children, opts)
  end

  defp maybe_indexer_children do
    if Application.get_env(:opake_indexer, :indexer_enabled, true) do
      [
        OpakeIndexer.TombstoneCleanup,
        OpakeIndexer.Jetstream.Consumer,
        OpakeIndexer.Firehose.Heartbeat,
        OpakeIndexer.Firehose.ConsumeLag
      ]
    else
      []
    end
  end

  @impl true
  def config_change(changed, _new, removed) do
    OpakeIndexerWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
