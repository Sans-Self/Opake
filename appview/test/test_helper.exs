Mox.defmock(OpakeAppview.Auth.KeyFetcherMock, for: OpakeAppview.Auth.KeyFetcherBehaviour)
Application.put_env(:opake_appview, :key_fetcher, OpakeAppview.Auth.KeyFetcherMock)

ExUnit.start()
Ecto.Adapters.SQL.Sandbox.mode(OpakeAppview.Repo, :manual)
