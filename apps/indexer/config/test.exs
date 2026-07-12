import Config

config :opake_indexer, OpakeIndexer.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "opake_indexer_test#{System.get_env("MIX_TEST_PARTITION")}",
  pool: Ecto.Adapters.SQL.Sandbox,
  # Core-scaled pooling grabs ~36 connections on a big machine and saturates
  # postgres (max_connections) when runs overlap; async DataCase throughput
  # plateaus well below that.
  pool_size: min(System.schedulers_online() * 2, 12)

config :opake_indexer, OpakeIndexerWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "qLFGZ/oib3gMumgPqHDETV3VM0klUHbVSFijSctQvjmTDRoshDjL+HyCbHS8IS3k",
  server: false

config :opake_indexer, :indexer_enabled, false

# Tests don't talk to Jetstream — they call Firehose.process_message
# directly. Make cursor saves immediate so the existing pipeline tests
# behave identically to before the time-based throttling.
config :opake_indexer, :cursor_save_interval_ms, 0
config :opake_indexer, :firehose_mode, {:custom, []}

config :logger, level: :warning
config :phoenix, :plug_init_mode, :runtime
