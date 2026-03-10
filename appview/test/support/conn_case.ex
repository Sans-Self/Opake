defmodule OpakeAppviewWeb.ConnCase do
  use ExUnit.CaseTemplate

  using do
    quote do
      @endpoint OpakeAppviewWeb.Endpoint

      import Plug.Conn
      import Phoenix.ConnTest
      import OpakeAppviewWeb.ConnCase
    end
  end

  setup tags do
    OpakeAppview.DataCase.setup_sandbox(tags)
    {:ok, conn: Phoenix.ConnTest.build_conn()}
  end

  @doc """
  Creates an authenticated conn with a fresh Ed25519 keypair and Mox expectation.
  Sets the `Opake-Ed25519` header for the given DID and path.
  """
  def authed_conn(conn, did, path) do
    {pubkey, privkey} = :crypto.generate_key(:eddsa, :ed25519)

    Mox.expect(OpakeAppview.Auth.KeyFetcherMock, :fetch_signing_key, fn ^did -> {:ok, pubkey} end)

    timestamp = System.system_time(:second)
    message = "GET:#{path}:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [privkey, :ed25519])
    sig_b64 = Base.encode64(signature)

    Plug.Conn.put_req_header(conn, "authorization", "Opake-Ed25519 #{did}:#{timestamp}:#{sig_b64}")
  end
end
