defmodule OpakeIndexerWeb.WorkspaceControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.{KeyringQueries, DirectoryQueries, DocumentQueries}

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  @keyring_uri "at://did:plc:owner/app.opake.keyring/3def"

  defp setup_workspace(did) do
    {:ok, _} =
      KeyringQueries.upsert_keyring(@keyring_uri, "did:plc:owner", [%{did: did, role: "manager"}])

    {:ok, _} =
      DirectoryQueries.upsert_directory(%{
        directory_uri: "at://did:plc:owner/app.opake.directory/3dir",
        keyring_uri: @keyring_uri,
        owner_did: "did:plc:owner",
        entries: ["at://did:plc:owner/app.opake.document/3doc"],
        encrypted_metadata: %{"ciphertext" => "AAAA"},
        key_wrapping: %{"$type" => "keyringKeyWrapping"},
        deleted_at: nil,
        indexed_at: DateTime.utc_now()
      })

    {:ok, _} =
      DocumentQueries.upsert_document(%{
        document_uri: "at://did:plc:owner/app.opake.document/3doc",
        keyring_uri: @keyring_uri,
        owner_did: "did:plc:owner",
        rotation: 0,
        encrypted_metadata: %{"ciphertext" => "BBBB"},
        encryption: %{"$type" => "keyringEncryption"},
        blob_ref: %{"$type" => "blob", "size" => 1024},
        deleted_at: nil,
        indexed_at: DateTime.utc_now()
      })
  end

  test "snapshotrequires keyring parameter", %{conn: conn} do
    did = "did:plc:me"
    conn = conn |> authed_conn(did, "/api/workspace/snapshot") |> get("/api/workspace/snapshot")

    response = json_response(conn, 400)
    assert response["error"] =~ "keyring"
  end

  test "snapshotrejects non-members", %{conn: conn} do
    did = "did:plc:outsider"

    {:ok, _} =
      KeyringQueries.upsert_keyring(@keyring_uri, "did:plc:owner", [
        %{did: "did:plc:alice", role: "manager"}
      ])

    conn =
      conn
      |> authed_conn(did, "/api/workspace/snapshot")
      |> get("/api/workspace/snapshot?keyring=#{URI.encode_www_form(@keyring_uri)}")

    response = json_response(conn, 403)
    assert response["error"] =~ "not a member"
  end

  test "snapshotreturns directories and documents", %{conn: conn} do
    did = "did:plc:me"
    setup_workspace(did)

    conn =
      conn
      |> authed_conn(did, "/api/workspace/snapshot")
      |> get("/api/workspace/snapshot?keyring=#{URI.encode_www_form(@keyring_uri)}")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
    assert length(response["documents"]) == 1
    assert is_binary(response["server_time"])

    dir = hd(response["directories"])
    assert dir["directory_uri"] == "at://did:plc:owner/app.opake.directory/3dir"
    assert dir["owner_did"] == "did:plc:owner"
    assert dir["entries"] == ["at://did:plc:owner/app.opake.document/3doc"]
    assert dir["encrypted_metadata"] == %{"ciphertext" => "AAAA"}

    doc = hd(response["documents"])
    assert doc["document_uri"] == "at://did:plc:owner/app.opake.document/3doc"
    assert doc["owner_did"] == "did:plc:owner"
    assert doc["rotation"] == 0
    assert doc["encrypted_metadata"] == %{"ciphertext" => "BBBB"}
  end

  test "sync requires keyring and since parameters", %{conn: conn} do
    did = "did:plc:me"

    {:ok, _} =
      KeyringQueries.upsert_keyring(@keyring_uri, "did:plc:owner", [%{did: did, role: "manager"}])

    conn =
      conn
      |> authed_conn(did, "/api/workspace/sync")
      |> get("/api/workspace/sync?keyring=#{URI.encode_www_form(@keyring_uri)}")

    response = json_response(conn, 400)
    assert response["error"] =~ "since"
  end

  test "sync returns changes since timestamp", %{conn: conn} do
    did = "did:plc:me"
    setup_workspace(did)

    since = DateTime.add(DateTime.utc_now(), -60, :second) |> DateTime.to_iso8601()

    conn =
      conn
      |> authed_conn(did, "/api/workspace/sync")
      |> get("/api/workspace/sync?keyring=#{URI.encode_www_form(@keyring_uri)}&since=#{since}")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
    assert length(response["documents"]) == 1
    assert is_binary(response["server_time"])
  end

  test "sync rejects invalid since timestamp", %{conn: conn} do
    did = "did:plc:me"

    {:ok, _} =
      KeyringQueries.upsert_keyring(@keyring_uri, "did:plc:owner", [%{did: did, role: "manager"}])

    conn =
      conn
      |> authed_conn(did, "/api/workspace/sync")
      |> get("/api/workspace/sync?keyring=#{URI.encode_www_form(@keyring_uri)}&since=not-a-date")

    response = json_response(conn, 400)
    assert response["error"] =~ "invalid since"
  end
end
