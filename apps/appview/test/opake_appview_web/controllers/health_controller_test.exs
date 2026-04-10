defmodule OpakeAppviewWeb.HealthControllerTest do
  use OpakeAppviewWeb.ConnCase, async: true

  alias OpakeAppview.Queries.GrantQueries

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
      GrantQueries.upsert_grant(%{
        uri: "at://did:plc:owner/app.opake.grant/3abc",
        owner_did: "did:plc:owner",
        recipient_did: "did:plc:me",
        document_uri: "at://did:plc:owner/app.opake.document/3xyz",
        created_at: "2026-03-01T12:00:00Z",
        indexed_at: DateTime.utc_now()
      })

    conn = get(conn, "/api/health")

    response = json_response(conn, 200)
    refute Map.has_key?(response, "grant_count")
    refute Map.has_key?(response, "keyring_count")
  end
end
