defmodule OpakeAppviewWeb.Router do
  @moduledoc """
  API router. Health is public; all other routes require Opake-Ed25519 auth.
  All routes are rate-limited per IP.

  NOTE TO EDITORS:
  Opake uses a dual-documentation system. If you modify the API surface,
  authentication schemes, or indexing logic in this service, you MUST also
  update the corresponding MDX content in `web/src/content/` to prevent
  documentation drift.
  """

  use OpakeAppviewWeb, :router

  pipeline :api do
    plug :accepts, ["json"]
    plug OpakeAppviewWeb.Plugs.RateLimit
  end

  pipeline :authenticated do
    plug OpakeAppview.Auth.Plug
  end

  # SSE stream — token-authenticated, outside the :authenticated pipeline.
  # Rate limiting excluded (long-lived connection, not a burst endpoint).
  scope "/api", OpakeAppviewWeb do
    get "/events", EventsController, :stream
  end

  scope "/api", OpakeAppviewWeb do
    pipe_through :api

    get "/health", HealthController, :index

    pipe_through :authenticated

    post "/events/token", EventsController, :create_token
    get "/inbox", InboxController, :index
    get "/keyrings", KeyringsController, :index

    get "/cabinet/snapshot", CabinetController, :snapshot
    get "/cabinet/sync", CabinetController, :sync

    get "/workspace", WorkspaceController, :documents
    get "/workspace/updates", WorkspaceController, :updates
    get "/workspace/directory-updates", WorkspaceController, :directory_updates
    get "/workspace/snapshot", WorkspaceController, :snapshot
    get "/workspace/sync", WorkspaceController, :sync
  end
end
