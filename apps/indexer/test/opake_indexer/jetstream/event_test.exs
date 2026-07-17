defmodule OpakeIndexer.Jetstream.EventTest do
  @moduledoc """
  Parser unit tests: Jetstream JSON in, dispatch tuple out. `derive_workspace_id`
  for a superseding keyring hits the DB (RecordQueries.lookup), so this suite
  needs the sandbox even though most cases are pure.
  """

  use OpakeIndexer.DataCase, async: true

  alias OpakeIndexer.Jetstream.Event
  alias OpakeIndexer.Queries.RecordQueries

  @alice "did:plc:alice"

  defp commit_json(did, operation, collection, rkey, opts \\ []) do
    commit =
      %{"operation" => operation, "collection" => collection, "rkey" => rkey}
      |> maybe_put("cid", Keyword.get(opts, :cid, "bafytest"))
      |> maybe_put("record", Keyword.get(opts, :record))

    Jason.encode!(%{"kind" => "commit", "did" => did, "time_us" => 123, "commit" => commit})
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  describe "malformed / unhandled frames" do
    test "invalid JSON ignores with nil time_us and collection" do
      assert {nil, nil, :ignore} = Event.parse("not json")
    end

    test "non-commit kind ignores but still extracts time_us" do
      json = Jason.encode!(%{"kind" => "identify", "time_us" => 42, "did" => @alice})
      assert {42, nil, :ignore} = Event.parse(json)
    end

    test "commit missing collection/operation/rkey ignores" do
      json =
        Jason.encode!(%{"kind" => "commit", "did" => @alice, "time_us" => 1, "commit" => %{}})

      assert {1, nil, :ignore} = Event.parse(json)
    end

    test "create on an indexed collection with no record body ignores" do
      json = commit_json(@alice, "create", "at.opake.document", "abc", record: nil)
      assert {123, "at.opake.document", :ignore} = Event.parse(json)
    end

    test "create on a collection outside at.opake.* ignores but preserves collection" do
      json = commit_json(@alice, "create", "app.bsky.feed.post", "abc", record: %{"text" => "hi"})
      assert {123, "app.bsky.feed.post", :ignore} = Event.parse(json)
    end
  end

  describe "upsert_record — keyring workspace_id derivation" do
    test "genesis keyring (no lineage, no supersedes) uses its own uri" do
      uri = "at://#{@alice}/at.opake.keyring/genesis"

      json =
        commit_json(@alice, "create", "at.opake.keyring", "genesis", record: %{"members" => []})

      assert {123, "at.opake.keyring", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.uri == uri
      assert attrs.workspace_id == uri
      assert attrs.supersedes_uri == nil
    end

    test "keyring with an explicit lineage uses it verbatim" do
      ws = "at://#{@alice}/at.opake.keyring/genesis"

      json =
        commit_json(@alice, "create", "at.opake.keyring", "head",
          record: %{"lineage" => ws, "supersedes" => ws}
        )

      assert {123, "at.opake.keyring", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == ws
      assert attrs.supersedes_uri == ws
    end

    test "keyring supersede with no lineage resolves via the indexed predecessor" do
      genesis = "at://#{@alice}/at.opake.keyring/genesis"

      {:ok, _} =
        RecordQueries.upsert(%{
          uri: genesis,
          collection: "at.opake.keyring",
          author_did: @alice,
          workspace_id: genesis,
          cid: "bafy-genesis",
          indexed_at: DateTime.utc_now(),
          record_jsonb: %{"members" => []}
        })

      json =
        commit_json(@alice, "create", "at.opake.keyring", "head",
          record: %{"supersedes" => genesis}
        )

      assert {123, "at.opake.keyring", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == genesis
    end

    test "keyring supersede whose predecessor isn't indexed yet resolves to nil (orphan)" do
      json =
        commit_json(@alice, "create", "at.opake.keyring", "head",
          record: %{"supersedes" => "at://never/indexed/rec"}
        )

      assert {123, "at.opake.keyring", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == nil
    end
  end

  describe "upsert_record — directory / document / grant" do
    test "workspace-scoped directory carries its workspaceId and root flag" do
      ws = "at://#{@alice}/at.opake.keyring/genesis"

      json =
        commit_json(@alice, "create", "at.opake.directory", "root",
          record: %{"workspaceId" => ws, "isWorkspaceRoot" => true}
        )

      assert {123, "at.opake.directory", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == ws
      assert attrs.is_workspace_root == true
    end

    test "cabinet directory (no isWorkspaceRoot field) defaults to not-root" do
      json = commit_json(@alice, "create", "at.opake.directory", "d1", record: %{})

      assert {123, "at.opake.directory", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == nil
      assert attrs.is_workspace_root == false
    end

    test "is_workspace_root is a pure function of the record's own field, independent of workspaceId" do
      json =
        commit_json(@alice, "create", "at.opake.directory", "d1",
          record: %{"isWorkspaceRoot" => true}
        )

      assert {123, "at.opake.directory", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.is_workspace_root == true
    end

    test "document never sets is_workspace_root regardless of the field" do
      json =
        commit_json(@alice, "create", "at.opake.document", "doc1",
          record: %{"isWorkspaceRoot" => true}
        )

      assert {123, "at.opake.document", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.is_workspace_root == false
    end

    test "grant carries no workspaceId field and derives nil" do
      json =
        commit_json(@alice, "create", "at.opake.grant", "g1",
          record: %{"recipient" => "did:plc:bob", "document" => "at://x/at.opake.document/y"}
        )

      assert {123, "at.opake.grant", {:upsert_record, attrs}} = Event.parse(json)
      assert attrs.workspace_id == nil
    end

    test "update operation follows the same upsert path as create" do
      json =
        commit_json(@alice, "update", "at.opake.document", "doc1",
          record: %{"encryptedMetadata" => "x"}
        )

      assert {123, "at.opake.document", {:upsert_record, _attrs}} = Event.parse(json)
    end
  end

  describe "delete_record" do
    for collection <- [
          "at.opake.grant",
          "at.opake.keyring",
          "at.opake.directory",
          "at.opake.document"
        ] do
      test "delete on #{collection} yields a delete_record tuple with the full uri" do
        collection = unquote(collection)
        uri = "at://#{@alice}/#{collection}/xyz"
        json = commit_json(@alice, "delete", collection, "xyz")

        assert {123, ^collection, {:delete_record, %{uri: ^uri}}} = Event.parse(json)
      end
    end
  end

  describe "account_config_seen" do
    test "create is reported as a heartbeat with op create" do
      json = commit_json(@alice, "create", "at.opake.accountConfig", "self", record: %{})

      assert {123, "at.opake.accountConfig",
              {:account_config_seen, %{did: @alice, op: "create"}}} =
               Event.parse(json)
    end

    test "update is reported with op update" do
      json = commit_json(@alice, "update", "at.opake.accountConfig", "self", record: %{})

      assert {123, "at.opake.accountConfig", {:account_config_seen, %{op: "update"}}} =
               Event.parse(json)
    end

    test "delete is reported with op delete" do
      json = commit_json(@alice, "delete", "at.opake.accountConfig", "self")

      assert {123, "at.opake.accountConfig", {:account_config_seen, %{op: "delete"}}} =
               Event.parse(json)
    end
  end
end
