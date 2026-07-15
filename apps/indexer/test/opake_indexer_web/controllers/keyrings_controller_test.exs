defmodule OpakeIndexerWeb.KeyringsControllerTest do
  use OpakeIndexerWeb.ConnCase, async: false
  import Mox

  alias OpakeIndexer.Queries.{ChainHeadQueries, RecordQueries}

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

  defp put_keyring(uri, members_dids, indexed_at) do
    {:ok, record} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: "at.opake.keyring",
        author_did: List.first(members_dids),
        workspace_id: uri,
        cid: "bafytest#{uri}",
        indexed_at: indexed_at,
        record_jsonb: %{
          "opakeVersion" => 1,
          "members" =>
            Enum.map(members_dids, fn did ->
              %{"wrappedKey" => %{"did" => did}, "role" => "manager"}
            end)
        }
      })

    {:ok, _} = ChainHeadQueries.create(uri, "keyring", uri, record.cid)
    record
  end

  describe "GET /api/keyrings" do
    # spec:workspace-membership § Membership state is the keyring head's member list
    test "returns no workspaces for a did that belongs to none", %{conn: conn} do
      did = "did:plc:nobody"
      conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

      assert json_response(conn, 200)["workspaces"] == []
    end

    # spec:workspace-membership § Membership state is the keyring head's member list
    test "returns the head keyring envelope for every workspace the did is a member of", %{
      conn: conn
    } do
      did = "did:plc:me"
      other_workspace_member = "did:plc:other"
      genesis = "at://did:plc:me/at.opake.keyring/genesis"
      other_genesis = "at://#{other_workspace_member}/at.opake.keyring/genesis"

      put_keyring(genesis, [did], DateTime.utc_now())
      put_keyring(other_genesis, [other_workspace_member], DateTime.utc_now())

      conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

      response = json_response(conn, 200)
      assert [envelope] = response["workspaces"]
      assert envelope["uri"] == genesis

      assert envelope["record"]["members"] == [
               %{"wrappedKey" => %{"did" => did}, "role" => "manager"}
             ]
    end

    test "reflects the current chain head, not a superseded record", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://did:plc:me/at.opake.keyring/genesis"
      head = "at://did:plc:me/at.opake.keyring/head"

      put_keyring(genesis, [did], DateTime.add(DateTime.utc_now(), -100, :second))

      {:ok, head_record} =
        RecordQueries.upsert(%{
          uri: head,
          collection: "at.opake.keyring",
          author_did: did,
          workspace_id: genesis,
          supersedes_uri: genesis,
          cid: "bafytest-head",
          indexed_at: DateTime.utc_now(),
          record_jsonb: %{
            "members" => [
              %{"wrappedKey" => %{"did" => did}, "role" => "manager"},
              %{"wrappedKey" => %{"did" => "did:plc:new"}, "role" => "editor"}
            ]
          }
        })

      {:ok, _} = ChainHeadQueries.advance(genesis, "keyring", head, head_record.cid, genesis)

      conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

      response = json_response(conn, 200)
      assert [envelope] = response["workspaces"]
      assert envelope["uri"] == head
      assert length(envelope["record"]["members"]) == 2
    end
  end
end
