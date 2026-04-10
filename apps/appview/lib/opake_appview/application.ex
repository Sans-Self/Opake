defmodule OpakeAppview.Application do
  @moduledoc """
  OTP application for the Opake AppView — a read-only Jetstream indexer and
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
    OpakeAppview.Indexer.init_state()
    OpakeAppview.Jetstream.Compression.init()
    OpakeAppview.SSE.TokenStore.init_table()
    OpakeAppview.SSE.ConnectionTracker.init_table()

    children =
      [
        OpakeAppview.Repo,
        {Phoenix.PubSub, name: OpakeAppview.PubSub},
        OpakeAppview.SSE.TokenStore,
        OpakeAppview.Auth.KeyCache,
        OpakeAppviewWeb.Endpoint
      ] ++
        maybe_indexer_children()

    opts = [strategy: :one_for_one, name: OpakeAppview.Supervisor]
    Supervisor.start_link(children, opts)
  end

  defp maybe_indexer_children do
    if Application.get_env(:opake_appview, :indexer_enabled, true) do
      [
        OpakeAppview.TombstoneCleanup,
        OpakeAppview.Jetstream.Consumer,
        OpakeAppview.Indexer.Heartbeat
      ]
    else
      []
    end
  end

  @impl true
  def config_change(changed, _new, removed) do
    OpakeAppviewWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
