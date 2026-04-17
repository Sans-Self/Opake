defmodule OpakeIndexer.Repo do
  use Ecto.Repo,
    otp_app: :opake_indexer,
    adapter: Ecto.Adapters.Postgres
end
