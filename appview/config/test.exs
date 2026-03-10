import Config

config :opake_appview, OpakeAppview.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "opake_appview_test#{System.get_env("MIX_TEST_PARTITION")}",
  pool: Ecto.Adapters.SQL.Sandbox,
  pool_size: System.schedulers_online() * 2

config :opake_appview, OpakeAppviewWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "qLFGZ/oib3gMumgPqHDETV3VM0klUHbVSFijSctQvjmTDRoshDjL+HyCbHS8IS3k",
  server: false

config :opake_appview, :indexer_enabled, false

config :logger, level: :warning
config :phoenix, :plug_init_mode, :runtime
