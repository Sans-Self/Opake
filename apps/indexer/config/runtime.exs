import Config

if System.get_env("PHX_SERVER") not in [nil, "false", "0"] do
  config :opake_indexer, OpakeIndexerWeb.Endpoint, server: true
end

if System.get_env("INDEXER_ENABLED") in ["false", "0"] do
  config :opake_indexer, :indexer_enabled, false
end

if jetstream_url = System.get_env("JETSTREAM_URL") do
  config :opake_indexer, :jetstream_url, jetstream_url
end

if cors_origin = System.get_env("CORS_ORIGIN") do
  config :opake_indexer, :cors_origin, cors_origin
end

if config_env() == :prod do
  database_url =
    System.get_env("DATABASE_URL") ||
      raise "environment variable DATABASE_URL is missing"

  maybe_ipv6 = if System.get_env("ECTO_IPV6") in ~w(true 1), do: [:inet6], else: []

  config :opake_indexer, OpakeIndexer.Repo,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    socket_options: maybe_ipv6

  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise "environment variable SECRET_KEY_BASE is missing"

  host = System.get_env("PHX_HOST") || "localhost"
  port = String.to_integer(System.get_env("PORT") || "6100")

  config :opake_indexer, OpakeIndexerWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [ip: {0, 0, 0, 0, 0, 0, 0, 0}, port: port],
    secret_key_base: secret_key_base
end
