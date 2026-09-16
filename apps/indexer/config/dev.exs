import Config

config :opake_indexer, OpakeIndexer.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "opake_indexer_dev",
  stacktrace: true,
  show_sensitive_data_on_connection_error: true,
  pool_size: 10

config :opake_indexer, OpakeIndexerWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: String.to_integer(System.get_env("PORT") || "6100")],
  check_origin: false,
  code_reloader: true,
  debug_errors: true,
  secret_key_base: "BFc2e5YOPLjLIEFGAPZ2kemmnuc7VOwv5ctKtiaYOX/r2bTLG8X2sVVcI6cYPjK1",
  watchers: []

config :opake_indexer,
  jetstream_url: "wss://jetstream1.eurosky.network/subscribe",
  firehose_mode: :opake_only,
  cors_origin: "*"

config :opake_indexer, dev_routes: true

config :logger, level: :info
config :logger, :default_formatter, format: "[$level] $message\n"
config :phoenix, :stacktrace_depth, 20
config :phoenix, :plug_init_mode, :runtime
