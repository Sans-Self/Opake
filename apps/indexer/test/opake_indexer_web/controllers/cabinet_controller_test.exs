defmodule OpakeIndexerWeb.CabinetControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.{DirectoryQueries, DocumentQueries}

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  defp setup_cabinet(did) do
    {:ok, _} =
      DirectoryQueries.upsert_directory(%{
        directory_uri: "at://#{did}/app.opake.directory/3dir",
        keyring_uri: nil,
        owner_did: did,
        entries: ["at://#{did}/app.opake.document/3doc"],
        encrypted_metadata: %{"ciphertext" => "AAAA"},
        key_wrapping: %{"$type" => "directKeyWrapping"},
        deleted_at: nil,
        indexed_at: DateTime.utc_now()
      })

    {:ok, _} =
      DocumentQueries.upsert_document(%{
        document_uri: "at://#{did}/app.opake.document/3doc",
        keyring_uri: nil,
        owner_did: did,
        rotation: nil,
        encrypted_metadata: %{"ciphertext" => "BBBB"},
        encryption: %{"$type" => "directEncryption"},
        blob_ref: %{"$type" => "blob", "size" => 512},
        deleted_at: nil,
        indexed_at: DateTime.utc_now()
      })
  end

  test "snapshotreturns cabinet directories and documents", %{conn: conn} do
    did = "did:plc:me"
    setup_cabinet(did)

    conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
    assert length(response["documents"]) == 1
    assert is_binary(response["server_time"])

    dir = hd(response["directories"])
    assert dir["directory_uri"] == "at://did:plc:me/app.opake.directory/3dir"
    assert dir["owner_did"] == "did:plc:me"
    refute Map.has_key?(dir, "keyring_uri")

    doc = hd(response["documents"])
    assert doc["document_uri"] == "at://did:plc:me/app.opake.document/3doc"
    refute Map.has_key?(doc, "keyring_uri")
  end

  test "snapshotexcludes workspace entries", %{conn: conn} do
    did = "did:plc:me"
    setup_cabinet(did)

    # Add a workspace directory — should NOT appear in cabinet tree
    {:ok, _} =
      DirectoryQueries.upsert_directory(%{
        directory_uri: "at://#{did}/app.opake.directory/3ws",
        keyring_uri: "at://#{did}/app.opake.keyring/3kr",
        owner_did: did,
        entries: [],
        encrypted_metadata: nil,
        key_wrapping: nil,
        deleted_at: nil,
        indexed_at: DateTime.utc_now()
      })

    conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
  end

  test "snapshotexcludes soft-deleted entries", %{conn: conn} do
    did = "did:plc:me"
    setup_cabinet(did)

    DirectoryQueries.soft_delete_directory(
      "at://#{did}/app.opake.directory/3dir",
      DateTime.utc_now()
    )

    conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

    response = json_response(conn, 200)
    assert response["directories"] == []
    assert length(response["documents"]) == 1
  end

  test "sync requires since parameter", %{conn: conn} do
    did = "did:plc:me"

    conn = conn |> authed_conn(did, "/api/cabinet/sync") |> get("/api/cabinet/sync")

    response = json_response(conn, 400)
    assert response["error"] =~ "since"
  end

  test "sync returns changes since timestamp", %{conn: conn} do
    did = "did:plc:me"
    setup_cabinet(did)

    since = DateTime.add(DateTime.utc_now(), -60, :second) |> DateTime.to_iso8601()

    conn =
      conn |> authed_conn(did, "/api/cabinet/sync") |> get("/api/cabinet/sync?since=#{since}")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
    assert length(response["documents"]) == 1
  end

  test "sync includes soft-deleted entries", %{conn: conn} do
    did = "did:plc:me"
    setup_cabinet(did)

    DirectoryQueries.soft_delete_directory(
      "at://#{did}/app.opake.directory/3dir",
      DateTime.utc_now()
    )

    since = DateTime.add(DateTime.utc_now(), -60, :second) |> DateTime.to_iso8601()

    conn =
      conn |> authed_conn(did, "/api/cabinet/sync") |> get("/api/cabinet/sync?since=#{since}")

    response = json_response(conn, 200)
    assert length(response["directories"]) == 1
    dir = hd(response["directories"])
    assert dir["deleted_at"] != nil
  end

  test "sync rejects invalid since timestamp", %{conn: conn} do
    did = "did:plc:me"

    conn = conn |> authed_conn(did, "/api/cabinet/sync") |> get("/api/cabinet/sync?since=garbage")

    response = json_response(conn, 400)
    assert response["error"] =~ "invalid since"
  end
end
