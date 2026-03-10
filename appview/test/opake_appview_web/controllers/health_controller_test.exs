defmodule OpakeAppviewWeb.HealthControllerTest do
  use OpakeAppviewWeb.ConnCase, async: true

  alias OpakeAppview.Queries.GrantQueries

  test "returns health status", %{conn: conn} do
    conn = get(conn, "/api/health")

    response = json_response(conn, 200)
    assert response["indexerConnected"] == false
    assert response["cursorTime"] == nil
    assert response["cursorAgeSecs"] == nil
  end

  test "health omits counts", %{conn: conn} do
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
    refute Map.has_key?(response, "grantCount")
    refute Map.has_key?(response, "keyringCount")
  end
end
