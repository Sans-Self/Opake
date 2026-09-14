defmodule OpakeIndexer.FirehoseKeyringDeleteTest do
  @moduledoc """
  Pipeline coverage for keyring delete outcome resolution: Jetstream
  delete JSON in, chain-head mutation + outcome-carrying SSE broadcast
  out. Pins the contract that only a head delete moves tracked state,
  and that the rollback target is the newest live record — not the
  tombstone's `supersedes` link.
  """

  use OpakeIndexer.DataCase, async: false

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Queries.{ChainHeadQueries, RecordQueries}
  alias OpakeIndexer.SSE.Topics

  @alice "did:plc:alice"
  @bob "did:plc:bob"

  @genesis_rkey "genesis"
  @genesis "at://#{@alice}/at.opake.keyring/#{@genesis_rkey}"

  defp keyring_jsonb(members_dids, extra) do
    Map.merge(
      %{
        "members" =>
          Enum.map(members_dids, fn did ->
            %{"did" => did, "role" => "manager"}
          end)
      },
      extra
    )
  end

  defp put_keyring(uri, opts) do
    {:ok, _} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: "at.opake.keyring",
        author_did: @alice,
        workspace_id: Keyword.get(opts, :workspace_id, @genesis),
        supersedes_uri: Keyword.get(opts, :supersedes),
        cid: "bafy-#{uri}",
        indexed_at: Keyword.fetch!(opts, :indexed_at),
        record_jsonb:
          keyring_jsonb(
            Keyword.get(opts, :members, [@alice]),
            Keyword.get(opts, :extra_jsonb, %{})
          )
      })
  end

  defp delete_json(uri) do
    ["at:", "", did, collection, rkey] = String.split(uri, "/")

    Jason.encode!(%{
      "kind" => "commit",
      "did" => did,
      "time_us" => 0,
      "commit" => %{"operation" => "delete", "collection" => collection, "rkey" => rkey}
    })
  end

  defp process_delete(uri), do: Firehose.process_message(delete_json(uri), 0)

  defp subscribe_workspace(workspace_id) do
    :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.workspace(workspace_id))
  end

  defp t(seconds_ago) do
    DateTime.add(DateTime.utc_now(), -seconds_ago, :second)
  end

  describe "keyring delete outcomes" do
    # spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome
    test "genesis delete on a superseded chain is unchanged" do
      head = "at://#{@alice}/at.opake.keyring/head"
      put_keyring(@genesis, indexed_at: t(100))
      put_keyring(head, supersedes: @genesis, indexed_at: t(50))
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", head, "bafy-#{head}")

      subscribe_workspace(@genesis)
      process_delete(@genesis)

      assert_receive {:sse_event, "at.opake.keyring:delete",
                      %{uri: @genesis, workspace_id: @genesis, outcome: "unchanged"}}

      refute_receive {:sse_event, "at.opake.keyring:upsert", _}
      assert %{head_uri: ^head} = ChainHeadQueries.get(@genesis, "keyring")
      assert %{deleted_at: %DateTime{}} = RecordQueries.lookup(@genesis)
    end

    # spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome
    test "superseded intermediate delete is unchanged" do
      mid = "at://#{@alice}/at.opake.keyring/mid"
      head = "at://#{@alice}/at.opake.keyring/head"
      put_keyring(@genesis, indexed_at: t(100))
      put_keyring(mid, supersedes: @genesis, indexed_at: t(50))
      put_keyring(head, supersedes: mid, indexed_at: t(10))
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", head, "bafy-#{head}")

      subscribe_workspace(@genesis)
      process_delete(mid)

      assert_receive {:sse_event, "at.opake.keyring:delete", %{uri: ^mid, outcome: "unchanged"}}
      assert %{head_uri: ^head} = ChainHeadQueries.get(@genesis, "keyring")
    end

    # spec:keyring-tombstones § Rollback restores the newest live record and re-broadcasts it
    test "head delete rolls back to the predecessor and re-broadcasts it" do
      head = "at://#{@alice}/at.opake.keyring/head"
      put_keyring(@genesis, indexed_at: t(100))
      put_keyring(head, supersedes: @genesis, indexed_at: t(50))
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", head, "bafy-#{head}")

      subscribe_workspace(@genesis)
      process_delete(head)

      assert_receive {:sse_event, "at.opake.keyring:delete",
                      %{uri: ^head, workspace_id: @genesis, outcome: "rolled_back"}}

      assert_receive {:sse_event, "at.opake.keyring:upsert",
                      %{uri: @genesis, record: %{"members" => _}}}

      assert %{head_uri: @genesis} = ChainHeadQueries.get(@genesis, "keyring")
    end

    # spec:keyring-tombstones § Rollback restores the newest live record and re-broadcasts it
    test "head delete with a purged intermediate rolls back to the newest live record" do
      # Chain genesis -> mid -> head, where mid's tombstone was purged:
      # its row no longer exists at all. head's supersedes link dangles.
      mid = "at://#{@alice}/at.opake.keyring/mid"
      head = "at://#{@alice}/at.opake.keyring/head"
      put_keyring(@genesis, indexed_at: t(100))
      put_keyring(head, supersedes: mid, indexed_at: t(10))
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", head, "bafy-#{head}")

      subscribe_workspace(@genesis)
      process_delete(head)

      assert_receive {:sse_event, "at.opake.keyring:delete",
                      %{uri: ^head, outcome: "rolled_back"}}

      assert %{head_uri: @genesis} = ChainHeadQueries.get(@genesis, "keyring")
    end

    # spec:keyring-tombstones § The indexer resolves every keyring delete to an outcome
    test "sole-record delete tears down the workspace's tracked chains" do
      put_keyring(@genesis, indexed_at: t(100))
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", @genesis, "bafy-#{@genesis}")

      root = "at://#{@alice}/at.opake.directory/root"
      {:ok, _} = ChainHeadQueries.create(@genesis, "workspace_root", root, "bafy-#{root}")

      subscribe_workspace(@genesis)
      process_delete(@genesis)

      assert_receive {:sse_event, "at.opake.keyring:delete",
                      %{uri: @genesis, workspace_id: @genesis, outcome: "torn_down"}}

      refute_receive {:sse_event, "at.opake.keyring:upsert", _}
      assert ChainHeadQueries.get(@genesis, "keyring") == nil
      assert ChainHeadQueries.get(@genesis, "workspace_root") == nil
    end

    test "delete fans out to the deleted record's members' personal topics" do
      put_keyring(@genesis, indexed_at: t(100), members: [@alice, @bob])
      {:ok, _} = ChainHeadQueries.create(@genesis, "keyring", @genesis, "bafy-#{@genesis}")

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.personal(@bob))
      process_delete(@genesis)

      assert_receive {:sse_event, "at.opake.keyring:delete", %{outcome: "torn_down"}}
    end

    test "orphan row delete carries its own URI as workspace identity" do
      orphan = "at://#{@alice}/at.opake.keyring/orphan"

      {:ok, _} =
        RecordQueries.upsert(%{
          uri: orphan,
          collection: "at.opake.keyring",
          author_did: @alice,
          workspace_id: nil,
          cid: "bafy-#{orphan}",
          indexed_at: t(10),
          record_jsonb: keyring_jsonb([@alice], %{"supersedes" => "at://never/indexed/rec"})
        })

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.workspace(orphan))
      process_delete(orphan)

      assert_receive {:sse_event, "at.opake.keyring:delete",
                      %{uri: ^orphan, workspace_id: ^orphan, outcome: "unchanged"}}
    end
  end
end
