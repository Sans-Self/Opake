defmodule OpakeIndexerWeb.WorkspaceControllerTest do
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

  # Seeds a genesis keyring (so `is_member?` has a head record to read) with
  # the given members, and registers its chain head.
  defp put_keyring(uri, members_dids) do
    {:ok, record} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: "app.opake.keyring",
        author_did: List.first(members_dids),
        workspace_id: uri,
        cid: "bafytest#{uri}",
        indexed_at: DateTime.utc_now(),
        record_jsonb: %{
          "members" =>
            Enum.map(members_dids, fn did ->
              %{"wrappedKey" => %{"did" => did}, "role" => "manager"}
            end)
        }
      })

    {:ok, _} = ChainHeadQueries.create(uri, "keyring", uri, record.cid)
    record
  end

  defp put_directory(uri, workspace_id, indexed_at, opts \\ []) do
    RecordQueries.upsert(%{
      uri: uri,
      collection: "app.opake.directory",
      author_did: Keyword.get(opts, :author_did, "did:plc:me"),
      workspace_id: workspace_id,
      is_workspace_root: Keyword.get(opts, :is_workspace_root, false),
      cid: "bafytest#{uri}",
      indexed_at: indexed_at,
      record_jsonb: %{}
    })
  end

  describe "GET /api/workspace/snapshot" do
    test "returns 400 without workspace_id", %{conn: conn} do
      did = "did:plc:me"

      conn =
        conn |> authed_conn(did, "/api/workspace/snapshot") |> get("/api/workspace/snapshot")

      assert json_response(conn, 400)["error"] =~ "workspace_id"
    end

    # spec:workspace-membership § Membership state is the keyring head's member list
    test "returns 403 for a did not on the keyring's member list", %{conn: conn} do
      genesis = "at://did:plc:owner/app.opake.keyring/genesis"
      put_keyring(genesis, ["did:plc:owner"])

      outsider = "did:plc:outsider"

      conn =
        conn
        |> authed_conn(outsider, "/api/workspace/snapshot")
        |> get("/api/workspace/snapshot?workspace_id=#{genesis}")

      assert json_response(conn, 403)["error"] =~ "not a member"
    end

    test "returns the workspace tree for a member, scoped to that workspace", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      other_genesis = "at://did:plc:other/app.opake.keyring/genesis"
      put_keyring(genesis, [did])
      put_keyring(other_genesis, ["did:plc:other"])

      now = DateTime.utc_now()

      {:ok, _} =
        put_directory("at://#{did}/app.opake.directory/root", genesis, now,
          is_workspace_root: true
        )

      {:ok, _} = put_directory("at://did:plc:other/app.opake.directory/root", other_genesis, now)

      conn =
        conn
        |> authed_conn(did, "/api/workspace/snapshot")
        |> get("/api/workspace/snapshot?workspace_id=#{genesis}")

      response = json_response(conn, 200)
      assert response["workspace_id"] == genesis
      assert [%{"uri" => "at://" <> _}] = response["directories"]
    end

    test "accepts camelCase workspaceId as well as workspace_id", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      put_keyring(genesis, [did])

      conn =
        conn
        |> authed_conn(did, "/api/workspace/snapshot")
        |> get("/api/workspace/snapshot?workspaceId=#{genesis}")

      assert json_response(conn, 200)["workspace_id"] == genesis
    end
  end

  describe "GET /api/workspace/sync" do
    test "returns 400 without since parameter", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      put_keyring(genesis, [did])

      conn =
        conn
        |> authed_conn(did, "/api/workspace/sync")
        |> get("/api/workspace/sync?workspace_id=#{genesis}")

      assert json_response(conn, 400)["error"] =~ "since"
    end

    # spec:workspace-membership § Membership state is the keyring head's member list
    test "checks membership before parsing since", %{conn: conn} do
      genesis = "at://did:plc:owner/app.opake.keyring/genesis"
      put_keyring(genesis, ["did:plc:owner"])

      conn =
        conn
        |> authed_conn("did:plc:outsider", "/api/workspace/sync")
        |> get("/api/workspace/sync?workspace_id=#{genesis}&since=garbage")

      assert json_response(conn, 403)
    end

    test "returns only records changed after since", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      put_keyring(genesis, [did])

      cutoff = DateTime.utc_now()
      old = DateTime.add(cutoff, -100, :second)
      fresh = DateTime.add(cutoff, 100, :second)

      {:ok, _} = put_directory("at://#{did}/app.opake.directory/old", genesis, old)
      {:ok, _} = put_directory("at://#{did}/app.opake.directory/fresh", genesis, fresh)

      conn =
        conn
        |> authed_conn(did, "/api/workspace/sync")
        |> get("/api/workspace/sync?workspace_id=#{genesis}&since=#{DateTime.to_iso8601(cutoff)}")

      response = json_response(conn, 200)
      assert [%{"uri" => uri}] = response["directories"]
      assert uri == "at://#{did}/app.opake.directory/fresh"
    end
  end

  describe "GET /api/workspace/chain-head" do
    # spec:workspace-membership § Membership state is the keyring head's member list
    test "returns 403 for a non-member", %{conn: conn} do
      genesis = "at://did:plc:owner/app.opake.keyring/genesis"
      put_keyring(genesis, ["did:plc:owner"])

      conn =
        conn
        |> authed_conn("did:plc:outsider", "/api/workspace/chain-head")
        |> get("/api/workspace/chain-head?workspace_id=#{genesis}")

      assert json_response(conn, 403)
    end

    test "returns the keyring head and nil root_directory when no root is tracked", %{
      conn: conn
    } do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      keyring = put_keyring(genesis, [did])

      conn =
        conn
        |> authed_conn(did, "/api/workspace/chain-head")
        |> get("/api/workspace/chain-head?workspace_id=#{genesis}")

      response = json_response(conn, 200)
      assert response["workspace_id"] == genesis
      assert response["keyring"]["head_uri"] == genesis
      assert response["keyring"]["head_cid"] == keyring.cid
      assert response["root_directory"] == nil
    end

    test "returns both keyring and root_directory heads once the root is tracked", %{conn: conn} do
      did = "did:plc:me"
      genesis = "at://#{did}/app.opake.keyring/genesis"
      root = "at://#{did}/app.opake.directory/root"
      put_keyring(genesis, [did])
      {:ok, _} = ChainHeadQueries.create(genesis, "workspace_root", root, "bafytest-root")

      conn =
        conn
        |> authed_conn(did, "/api/workspace/chain-head")
        |> get("/api/workspace/chain-head?workspace_id=#{genesis}")

      response = json_response(conn, 200)
      assert response["root_directory"]["head_uri"] == root
    end
  end
end
