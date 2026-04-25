defmodule OpakeIndexerWeb.InboxControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.GrantQueries

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  test "requires did parameter", %{conn: conn} do
    did = "did:plc:test"
    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox")

    response = json_response(conn, 400)
    assert response["error"] =~ "did"
  end

  test "returns empty for unknown did", %{conn: conn} do
    did = "did:plc:nobody"
    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox?did=#{did}")

    response = json_response(conn, 200)
    assert response["grants"] == []
    refute Map.has_key?(response, "cursor")
  end

  test "returns grants for recipient", %{conn: conn} do
    did = "did:plc:me"

    {:ok, _} =
      GrantQueries.upsert_grant(%{
        uri: "at://did:plc:owner/app.opake.grant/3abc",
        owner_did: "did:plc:owner",
        recipient_did: did,
        document_uri: "at://did:plc:owner/app.opake.document/3xyz",
        created_at: "2026-03-01T12:00:00Z",
        indexed_at: DateTime.utc_now()
      })

    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox?did=#{did}")

    response = json_response(conn, 200)
    assert length(response["grants"]) == 1

    grant = hd(response["grants"])
    assert grant["owner_did"] == "did:plc:owner"
    assert grant["document_uri"] == "at://did:plc:owner/app.opake.document/3xyz"
  end

  test "pagination", %{conn: conn} do
    did = "did:plc:me"

    for i <- 0..4 do
      indexed_at =
        DateTime.new!(~D[2026-03-01], Time.new!(12, 0, i))
        |> DateTime.from_naive!("Etc/UTC")

      {:ok, _} =
        GrantQueries.upsert_grant(%{
          uri: "at://did:plc:owner/app.opake.grant/#{i}",
          owner_did: "did:plc:owner",
          recipient_did: did,
          document_uri: "at://did:plc:owner/app.opake.document/#{i}",
          created_at: "2026-03-01T12:00:00Z",
          indexed_at: indexed_at
        })
    end

    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox?did=#{did}&limit=3")

    response = json_response(conn, 200)
    assert length(response["grants"]) == 3
    assert is_binary(response["cursor"])
  end
end
