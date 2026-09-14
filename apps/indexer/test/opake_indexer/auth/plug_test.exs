defmodule OpakeIndexer.Auth.PlugTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  # Each test gets its own source IP so the per-IP rate limiter (shared
  # across the whole suite run, not per-file) never trips on a burst of
  # requests from this file alone.
  setup %{conn: conn} do
    ip = "10.0.1.#{System.unique_integer([:positive])}"
    {:ok, conn: Plug.Conn.put_req_header(conn, "x-forwarded-for", ip)}
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

  # A decision verified against the DID document's #opake anchor is accepted
  # and surfaced on the conn, through the real cache and plug.
  test "accepts an account verified against its #opake anchor", %{conn: conn} do
    did = "did:plc:verified"
    {pubkey, privkey} = :crypto.generate_key(:eddsa, :ed25519)
    timestamp = System.system_time(:second)
    message = "GET:/api/inbox:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [privkey, :ed25519])

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_authentication_decision, fn ^did ->
      {:ok, %{key: pubkey, verified: true, anchor_history: :not_replaced}}
    end)

    conn =
      conn
      |> put_req_header(
        "authorization",
        "Opake-Ed25519 #{did}:#{timestamp}:#{Base.encode64(signature)}"
      )
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 200)
    assert conn.assigns.authenticated_did == did
    assert conn.assigns.authenticated_verified
    assert conn.assigns.authenticated_anchor_history == :not_replaced
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

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_authentication_decision, fn ^did ->
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

  # An account that declares #opake but whose published record is not signed
  # under it is refused through the real cache and plug, not just at the
  # resolver boundary.
  test "refuses an anchored-but-unsigned account with a sanitized 401", %{conn: conn} do
    did = "did:plc:invalid-decision"
    {_pubkey, private_key} = :crypto.generate_key(:eddsa, :ed25519)
    timestamp = System.system_time(:second)
    message = "GET:/api/inbox:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [private_key, :ed25519])

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_authentication_decision, fn ^did ->
      {:error, {:invalid, :account_public_key_signature}}
    end)

    conn =
      conn
      |> put_req_header(
        "authorization",
        "Opake-Ed25519 #{did}:#{timestamp}:#{Base.encode64(signature)}"
      )
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 401) == %{"error" => "authentication key resolution invalid"}
  end

  test "returns 503 for DID or PDS transport failure without internal details", %{conn: conn} do
    did = "did:plc:unavailable"
    {_pubkey, private_key} = :crypto.generate_key(:eddsa, :ed25519)
    timestamp = System.system_time(:second)
    message = "GET:/api/inbox:#{timestamp}:#{did}"
    signature = :crypto.sign(:eddsa, :none, message, [private_key, :ed25519])

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_authentication_decision, fn ^did ->
      {:error, {:unavailable, :did_document}}
    end)

    conn =
      conn
      |> put_req_header(
        "authorization",
        "Opake-Ed25519 #{did}:#{timestamp}:#{Base.encode64(signature)}"
      )
      |> get("/api/inbox?did=#{did}")

    assert json_response(conn, 503) == %{"error" => "authentication key resolution unavailable"}
  end

  test "authentication reuses a complete decision until its short expiry" do
    did = "did:plc:changed-anchor"
    {pubkey, _private_key} = :crypto.generate_key(:eddsa, :ed25519)

    Mox.expect(OpakeIndexer.Auth.KeyFetcherMock, :fetch_authentication_decision, fn ^did ->
      {:ok, %{key: pubkey, verified: true, anchor_history: :not_replaced}}
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
