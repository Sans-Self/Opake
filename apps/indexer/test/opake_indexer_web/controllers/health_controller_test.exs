defmodule OpakeIndexerWeb.HealthControllerTest do
  use OpakeIndexerWeb.ConnCase, async: true

  alias OpakeIndexer.Queries.RecordQueries

  test "returns health status", %{conn: conn} do
    conn = get(conn, "/api/health")

    response = json_response(conn, 200)
    assert response["indexer_connected"] == false
    assert response["cursor_time"] == nil
    assert response["cursor_age_secs"] == nil

    # New fields surfaced by the indexer state ETS table — present even
    # when the indexer is disabled.
    assert is_map(response["events"])
    assert is_integer(response["events"]["total"])
    assert is_integer(response["events"]["indexed"])
    assert is_integer(response["events"]["ignored"])
    assert is_map(response["per_collection"])
  end

  test "health omits row counts (those are internal metrics)", %{conn: conn} do
    {:ok, _} =
      RecordQueries.upsert(%{
        uri: "at://did:plc:owner/app.opake.grant/3abc",
        collection: "app.opake.grant",
        author_did: "did:plc:owner",
        workspace_id: nil,
        supersedes_uri: nil,
        is_workspace_root: false,
        cid: "bafytest",
        indexed_at: DateTime.utc_now(),
        deleted_at: nil,
        record_jsonb: %{
          "opakeVersion" => 1,
          "document" => "at://did:plc:owner/app.opake.document/3xyz",
          "recipient" => "did:plc:me",
          "createdAt" => "2026-03-01T12:00:00Z"
        }
      })

    conn = get(conn, "/api/health")

    response = json_response(conn, 200)
    refute Map.has_key?(response, "grant_count")
    refute Map.has_key?(response, "keyring_count")
  end
end
