import Config

config :opake_appview,
  ecto_repos: [OpakeAppview.Repo],
  generators: [timestamp_type: :utc_datetime_usec]

config :opake_appview, OpakeAppviewWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [json: OpakeAppviewWeb.ErrorJSON],
    layout: false
  ]

config :logger, :default_formatter,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

config :phoenix, :json_library, Jason

config :hammer,
  backend:
    {Hammer.Backend.ETS, [expiry_ms: :timer.minutes(2), cleanup_interval_ms: :timer.minutes(1)]}

import_config "#{config_env()}.exs"
