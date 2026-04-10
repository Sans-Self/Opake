import Config

config :opake_appview,
  ecto_repos: [OpakeAppview.Repo],
  generators: [timestamp_type: :utc_datetime_usec],
  # Subscription mode for the Jetstream consumer.
  # See OpakeAppview.Jetstream.Consumer for the full set of options.
  firehose_mode: :full,
  # zstd-compressed binary frames using the vendored bsky dictionary.
  # Set to :none to receive raw JSON instead (debugging only).
  compression: :zstd,
  # Persist the cursor to Postgres at most once per N milliseconds, no
  # matter how many events flow through the indexer in between.
  cursor_save_interval_ms: 5_000

config :opake_appview, OpakeAppviewWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  # SSE connections are long-lived — raise the idle timeout so keepalive
  # chunks (every 15s) don't race the default limit.
  thousand_island_options: [read_timeout: 86_400_000],
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
