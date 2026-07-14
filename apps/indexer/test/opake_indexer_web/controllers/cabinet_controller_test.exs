defmodule OpakeIndexerWeb.CabinetControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.RecordQueries

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
    ip = "10.0.0.#{System.unique_integer([:positive])}"
    {:ok, conn: Plug.Conn.put_req_header(conn, "x-forwarded-for", ip)}
  end

  defp put_directory(uri, author, indexed_at) do
    RecordQueries.upsert(%{
      uri: uri,
      collection: "app.opake.directory",
      author_did: author,
      workspace_id: nil,
      is_workspace_root: false,
      cid: "bafytest#{uri}",
      indexed_at: indexed_at,
      record_jsonb: %{"opakeVersion" => 1, "encryptedMetadata" => "cipher"}
    })
  end

  defp put_document(uri, author, indexed_at) do
    RecordQueries.upsert(%{
      uri: uri,
      collection: "app.opake.document",
      author_did: author,
      workspace_id: nil,
      cid: "bafytest#{uri}",
      indexed_at: indexed_at,
      record_jsonb: %{"opakeVersion" => 1, "encryptedMetadata" => "cipher"}
    })
  end

  describe "GET /api/cabinet/snapshot" do
    test "returns empty tree for a did with no cabinet records", %{conn: conn} do
      did = "did:plc:nobody"
      conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

      response = json_response(conn, 200)
      assert response["directories"] == []
      assert response["documents"] == []
      assert is_binary(response["server_time"])
    end

    test "returns only the authenticated did's cabinet records", %{conn: conn} do
      did = "did:plc:me"
      other = "did:plc:other"
      now = DateTime.utc_now()

      {:ok, _} = put_directory("at://#{did}/app.opake.directory/d1", did, now)
      {:ok, _} = put_document("at://#{did}/app.opake.document/doc1", did, now)
      {:ok, _} = put_directory("at://#{other}/app.opake.directory/d2", other, now)

      conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

      response = json_response(conn, 200)
      assert length(response["directories"]) == 1
      assert length(response["documents"]) == 1
      assert [%{"uri" => "at://" <> _}] = response["directories"]
    end

    test "excludes workspace-scoped records even when authored by the same did", %{conn: conn} do
      did = "did:plc:me"
      now = DateTime.utc_now()

      {:ok, _} =
        RecordQueries.upsert(%{
          uri: "at://#{did}/app.opake.directory/wsroot",
          collection: "app.opake.directory",
          author_did: did,
          workspace_id: "at://#{did}/app.opake.keyring/genesis",
          is_workspace_root: true,
          cid: "bafytest-wsroot",
          indexed_at: now,
          record_jsonb: %{}
        })

      conn = conn |> authed_conn(did, "/api/cabinet/snapshot") |> get("/api/cabinet/snapshot")

      assert json_response(conn, 200)["directories"] == []
    end
  end

  describe "GET /api/cabinet/sync" do
    test "returns 400 without since parameter", %{conn: conn} do
      did = "did:plc:me"
      conn = conn |> authed_conn(did, "/api/cabinet/sync") |> get("/api/cabinet/sync")

      assert json_response(conn, 400)["error"] =~ "since"
    end

    test "returns 400 for an unparseable since timestamp", %{conn: conn} do
      did = "did:plc:me"

      conn =
        conn
        |> authed_conn(did, "/api/cabinet/sync")
        |> get("/api/cabinet/sync?since=not-a-timestamp")

      assert json_response(conn, 400)["error"] =~ "since"
    end

    test "returns only records changed after the since timestamp", %{conn: conn} do
      did = "did:plc:me"
      cutoff = DateTime.utc_now()
      old = DateTime.add(cutoff, -100, :second)
      fresh = DateTime.add(cutoff, 100, :second)

      {:ok, _} = put_document("at://#{did}/app.opake.document/old", did, old)
      {:ok, _} = put_document("at://#{did}/app.opake.document/fresh", did, fresh)

      conn =
        conn
        |> authed_conn(did, "/api/cabinet/sync")
        |> get("/api/cabinet/sync?since=#{DateTime.to_iso8601(cutoff)}")

      response = json_response(conn, 200)
      assert [%{"uri" => uri}] = response["documents"]
      assert uri == "at://#{did}/app.opake.document/fresh"
    end
  end
end
