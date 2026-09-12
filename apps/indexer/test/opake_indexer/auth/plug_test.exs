defmodule OpakeIndexer.Auth.PlugTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  test "rejects missing authorization header", %{conn: conn} do
    conn = get(conn, "/api/inbox?did=did:plc:test")

    assert json_response(conn, 401)["error"] =~ "authorization"
  end

  test "rejects bearer token auth", %{conn: conn} do
    conn =
      conn
      |> put_req_header("authorization", "Bearer some-token")
      |> get("/api/inbox?did=did:plc:test")

    assert json_response(conn, 401)["error"] =~ "scheme"
  end

  test "rejects basic auth", %{conn: conn} do
    conn =
      conn
      |> put_req_header("authorization", "Basic dXNlcjpwYXNz")
      |> get("/api/inbox?did=did:plc:test")

    assert json_response(conn, 401)["error"] =~ "scheme"
  end

  test "accepts valid ed25519 signed request", %{conn: conn} do
    did = "did:plc:testuser"

    conn =
      conn
      |> authed_conn(did, "/api/inbox")
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 200)
  end

  # An account without a DID-document #opake method keeps the existing
  # record-signing-key authentication path.
  test "accepts an unverified account", %{conn: conn} do
    did = "did:plc:unverified"
    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox?did=#{did}")
    assert json_response(conn, 200)
  end

  test "refuses an account whose anchored public-key record is invalid", %{conn: conn} do
    did = "did:plc:anchored"
    {_pubkey, private_key} = :crypto.generate_key(:eddsa, :ed25519)
    timestamp = System.system_time(:second)
    message = "GET:/api/inbox:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [private_key, :ed25519])

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_signing_key, fn ^did ->
      {:error, "invalid account public-key signature"}
    end)

    conn =
      conn
      |> put_req_header(
        "authorization",
        "Opake-Ed25519 #{did}:#{timestamp}:#{Base.encode64(signature)}"
      )
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 401)["error"] =~ "signature"
  end

  test "authentication never reuses a prior DID verification decision" do
    did = "did:plc:changed-anchor"
    {pubkey, _private_key} = :crypto.generate_key(:eddsa, :ed25519)

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_signing_key, 2, fn ^did ->
      {:ok, pubkey}
    end)

    assert {:ok, ^pubkey} = OpakeIndexer.Auth.KeyCache.get_key(did)
    assert {:ok, ^pubkey} = OpakeIndexer.Auth.KeyCache.get_key(did)
  end

  test "rejects expired timestamp", %{conn: conn} do
    old_timestamp = System.system_time(:second) - 120
    did = "did:plc:testuser"

    {_pubkey, privkey} = :crypto.generate_key(:eddsa, :ed25519)
    message = "GET:/api/inbox:#{old_timestamp}:#{did}"

    signature = :crypto.sign(:eddsa, :none, message, [privkey, :ed25519])
    sig_b64 = Base.encode64(signature)

    conn =
      conn
      |> put_req_header("authorization", "Opake-Ed25519 #{did}:#{old_timestamp}:#{sig_b64}")
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 401)["error"] =~ "drift"
  end

  test "rejects DID scope mismatch", %{conn: conn} do
    did = "did:plc:testuser"

    # DID scope check happens before key fetch, so no mock expectation needed.
    # Use a manually-signed request since authed_conn sets up a mock.
    {_pubkey, privkey} = :crypto.generate_key(:eddsa, :ed25519)
    timestamp = System.system_time(:second)
    message = "GET:/api/inbox:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [privkey, :ed25519])
    sig_b64 = Base.encode64(signature)

    conn =
      conn
      |> put_req_header("authorization", "Opake-Ed25519 #{did}:#{timestamp}:#{sig_b64}")
      |> get("/api/inbox?did=did:plc:other")

    assert json_response(conn, 401)["error"] =~ "mismatch"
  end

  test "health endpoint works without auth", %{conn: conn} do
    conn = get(conn, "/api/health")

    response = json_response(conn, 200)
    assert is_boolean(response["indexer_connected"])
    assert Map.has_key?(response, "cursor_time")
    assert Map.has_key?(response, "cursor_age_secs")
    refute Map.has_key?(response, "grant_count")
    refute Map.has_key?(response, "keyring_count")
  end
end
