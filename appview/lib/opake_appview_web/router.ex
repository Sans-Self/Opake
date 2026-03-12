defmodule OpakeAppviewWeb.Router do
  @moduledoc """
  API router. Health is public; inbox and keyrings require Opake-Ed25519 auth.
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

  scope "/api", OpakeAppviewWeb do
    pipe_through :api

    get "/health", HealthController, :index

    pipe_through :authenticated

    get "/inbox", InboxController, :index
    get "/keyrings", KeyringsController, :index
  end
end
