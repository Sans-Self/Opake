defmodule OpakeIndexerWeb.Router do
  @moduledoc """
  API router. Health is public; all other routes require Opake-Ed25519 auth.
  All routes are rate-limited per IP.

  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the API surface,
  authentication schemes, or indexing logic in this service, you MUST also
  update the corresponding MDX content in `apps/web/src/content/` to prevent
  documentation drift.
  """

  use OpakeIndexerWeb, :router

  pipeline :api do
    plug :accepts, ["json"]
    plug OpakeIndexerWeb.Plugs.RateLimit
  end

  pipeline :authenticated do
    plug OpakeIndexer.Auth.Plug
  end

  # SSE stream — token-authenticated, outside the :authenticated pipeline.
  # Rate limiting excluded (long-lived connection, not a burst endpoint).
  scope "/api", OpakeIndexerWeb do
    get "/events", EventsController, :stream
  end

  scope "/api", OpakeIndexerWeb do
    pipe_through :api

    get "/health", HealthController, :index

    pipe_through :authenticated

    post "/events/token", EventsController, :create_token
    get "/inbox", InboxController, :index
    get "/keyrings", KeyringsController, :index

    get "/cabinet/snapshot", CabinetController, :snapshot
    get "/cabinet/sync", CabinetController, :sync

    get "/workspace/snapshot", WorkspaceController, :snapshot
    get "/workspace/sync", WorkspaceController, :sync
    get "/workspace/chain-head", WorkspaceController, :chain_head
  end
end
