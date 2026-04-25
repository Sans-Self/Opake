defmodule OpakeIndexerWeb.Endpoint do
  use Phoenix.Endpoint, otp_app: :opake_indexer

  if code_reloading? do
    plug Phoenix.CodeReloader
    plug Phoenix.Ecto.CheckRepoStatus, otp_app: :opake_indexer
  end

  plug Plug.RequestId
  plug Plug.Telemetry, event_prefix: [:phoenix, :endpoint]

  plug OpakeIndexerWeb.Plugs.CORS

  plug Plug.Parsers,
    parsers: [:urlencoded, :json],
    pass: ["*/*"],
    json_decoder: Phoenix.json_library()

  plug OpakeIndexerWeb.Router
end
