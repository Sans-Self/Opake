defmodule OpakeIndexerWeb.InboxControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.RecordQueries

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  defp insert_grant(uri, author, recipient, document, indexed_at) do
    RecordQueries.upsert(%{
      uri: uri,
      collection: "at.opake.grant",
      author_did: author,
      workspace_id: nil,
      supersedes_uri: nil,
      is_workspace_root: false,
      cid: "bafytest#{uri}",
      indexed_at: indexed_at,
      deleted_at: nil,
      record_jsonb: %{
        "opakeVersion" => 1,
        "document" => document,
        "recipient" => recipient,
        "createdAt" => "2026-03-01T12:00:00Z"
      }
    })
  end

  test "returns empty for unknown did", %{conn: conn} do
    did = "did:plc:nobody"
    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox")

    response = json_response(conn, 200)
    assert response["grants"] == []
    refute Map.has_key?(response, "cursor")
  end

  test "returns grants for recipient", %{conn: conn} do
    did = "did:plc:me"

    {:ok, _} =
      insert_grant(
        "at://did:plc:owner/at.opake.grant/3abc",
        "did:plc:owner",
        did,
        "at://did:plc:owner/at.opake.document/3xyz",
        DateTime.utc_now()
      )

    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox")

    response = json_response(conn, 200)
    assert length(response["grants"]) == 1

    [envelope] = response["grants"]
    assert envelope["record"]["document"] == "at://did:plc:owner/at.opake.document/3xyz"
    assert envelope["record"]["recipient"] == did
    assert is_binary(envelope["indexedAt"])
  end

  test "pagination", %{conn: conn} do
    did = "did:plc:me"

    for i <- 0..4 do
      indexed_at =
        DateTime.new!(~D[2026-03-01], Time.new!(12, 0, i))
        |> DateTime.from_naive!("Etc/UTC")

      {:ok, _} =
        insert_grant(
          "at://did:plc:owner/at.opake.grant/#{i}",
          "did:plc:owner",
          did,
          "at://did:plc:owner/at.opake.document/#{i}",
          indexed_at
        )
    end

    conn = conn |> authed_conn(did, "/api/inbox") |> get("/api/inbox?limit=3")

    response = json_response(conn, 200)
    assert length(response["grants"]) == 3
    assert is_binary(response["cursor"])
  end
end
