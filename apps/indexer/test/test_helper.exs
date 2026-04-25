Mox.defmock(OpakeIndexer.Auth.KeyFetcherMock, for: OpakeIndexer.Auth.KeyFetcherBehaviour)
Application.put_env(:opake_indexer, :key_fetcher, OpakeIndexer.Auth.KeyFetcherMock)

ExUnit.start()
Ecto.Adapters.SQL.Sandbox.mode(OpakeIndexer.Repo, :manual)
