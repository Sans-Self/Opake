import Config

config :opake_appview, OpakeAppview.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "opake_appview_dev",
  stacktrace: true,
  show_sensitive_data_on_connection_error: true,
  pool_size: 10

config :opake_appview, OpakeAppviewWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: String.to_integer(System.get_env("PORT") || "6100")],
  check_origin: false,
  code_reloader: true,
  debug_errors: true,
  secret_key_base: "BFc2e5YOPLjLIEFGAPZ2kemmnuc7VOwv5ctKtiaYOX/r2bTLG8X2sVVcI6cYPjK1",
  watchers: []

config :opake_appview,
  jetstream_url: "wss://frankfurt.firehose.stream/tap",
  cors_origin: "*"

config :opake_appview, dev_routes: true

config :logger, :default_formatter, format: "[$level] $message\n"
config :phoenix, :stacktrace_depth, 20
config :phoenix, :plug_init_mode, :runtime
