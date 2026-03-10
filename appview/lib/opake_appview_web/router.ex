defmodule OpakeAppviewWeb.Router do
  @moduledoc """
  API router. Health is public; inbox and keyrings require Opake-Ed25519 auth.
  All routes are rate-limited per IP.
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
