defmodule OpakeIndexerWeb.EventsControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false

  import Mox
  setup :set_mox_global
  setup :verify_on_exit!

  alias OpakeIndexer.SSE.TokenStore

  @did "did:plc:test123"

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  describe "POST /api/events/token" do
    test "returns a token for authenticated user", %{conn: conn} do
      conn =
        conn
        |> authed_conn_post(@did, "/api/events/token")
        |> post("/api/events/token")

      assert %{"token" => token, "ttl" => ttl} = json_response(conn, 200)
      assert is_binary(token)
      assert ttl > 0
    end

    test "returns 401 without auth", %{conn: conn} do
      conn = post(conn, "/api/events/token")
      assert conn.status == 401
    end
  end

  describe "GET /api/events" do
    test "returns 400 without token", %{conn: conn} do
      conn = get(conn, "/api/events")
      assert json_response(conn, 400)["error"] =~ "token"
    end

    test "returns 401 with invalid token", %{conn: conn} do
      conn = get(conn, "/api/events?token=bogus")
      assert json_response(conn, 401)["error"] =~ "invalid"
    end

    test "returns 401 with consumed token", %{conn: conn} do
      token = TokenStore.create_token(@did)
      {:ok, _} = TokenStore.consume_token(token)

      conn = get(conn, "/api/events?token=#{token}")
      assert json_response(conn, 401)
    end
  end

  # Like authed_conn but for POST method
  defp authed_conn_post(conn, did, path) do
    {pubkey, privkey} = :crypto.generate_key(:eddsa, :ed25519)

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_signing_key, fn ^did -> {:ok, pubkey} end)

    timestamp = System.system_time(:second)
    message = "POST:#{path}:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [privkey, :ed25519])
    sig_b64 = Base.encode64(signature)

    Plug.Conn.put_req_header(
      conn,
      "authorization",
      "Opake-Ed25519 #{did}:#{timestamp}:#{sig_b64}"
    )
  end
end
