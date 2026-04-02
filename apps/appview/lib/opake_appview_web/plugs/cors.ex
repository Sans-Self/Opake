defmodule OpakeAppviewWeb.Plugs.CORS do
  @moduledoc """
  Minimal CORS plug. Allowed origin is configured per-environment:

      config :opake_appview, :cors_origin, "*"          # dev
      config :opake_appview, :cors_origin, "https://opake.app"  # prod

  Handles OPTIONS preflight and sets headers on all responses.
  """

  import Plug.Conn

  @behaviour Plug

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    origin = Application.get_env(:opake_appview, :cors_origin, "*")

    conn =
      conn
      |> put_resp_header("access-control-allow-origin", origin)
      |> put_resp_header("access-control-allow-methods", "GET, OPTIONS")
      |> put_resp_header("access-control-allow-headers", "authorization, content-type")
      |> put_resp_header("access-control-max-age", "86400")

    if conn.method == "OPTIONS" do
      conn |> send_resp(204, "") |> halt()
    else
      conn
    end
  end
end
