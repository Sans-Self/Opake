defmodule OpakeIndexer.SSE.BroadcasterTest do
  use ExUnit.Case, async: true

  alias OpakeIndexer.SSE.Broadcaster

  @pubsub OpakeIndexer.PubSub

  setup do
    # Ensure PubSub is available (started by Application)
    :ok
  end

  describe "broadcast_directory/2" do
    test "routes cabinet directory to did: topic" do
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal("did:plc:owner"))

      Broadcaster.broadcast_directory(
        %{
          directory_uri: "at://did:plc:owner/app.opake.directory/3dir",
          owner_did: "did:plc:owner",
          keyring_uri: nil,
          entries: ["at://did:plc:owner/app.opake.document/3a"],
          indexed_at: DateTime.utc_now()
        },
        :upsert
      )

      assert_receive {:sse_event, "directory:upsert", payload}
      assert payload.directory_uri == "at://did:plc:owner/app.opake.directory/3dir"
    end

    test "routes workspace directory to keyring: topic" do
      keyring = "at://did:plc:owner/app.opake.keyring/3kr"
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.workspace(keyring))

      Broadcaster.broadcast_directory(
        %{
          directory_uri: "at://did:plc:owner/app.opake.directory/3dir",
          owner_did: "did:plc:owner",
          keyring_uri: keyring,
          entries: [],
          indexed_at: DateTime.utc_now()
        },
        :upsert
      )

      assert_receive {:sse_event, "directory:upsert", payload}
      assert payload.directory_uri == "at://did:plc:owner/app.opake.directory/3dir"
    end

    test "delete broadcasts directory_uri" do
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal("did:plc:owner"))

      Broadcaster.broadcast_directory(
        %{
          directory_uri: "at://did:plc:owner/app.opake.directory/3dir",
          owner_did: "did:plc:owner",
          keyring_uri: nil
        },
        :delete
      )

      assert_receive {:sse_event, "directory:delete", %{directory_uri: _}}
    end
  end

  describe "broadcast_keyring/2" do
    test "broadcasts to both keyring: and did: topics" do
      uri = "at://did:plc:owner/app.opake.keyring/3kr"
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.workspace(uri))
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal("did:plc:owner"))

      Broadcaster.broadcast_keyring(
        %{
          uri: uri,
          owner_did: "did:plc:owner",
          rotation: 0,
          member_entries: [],
          indexed_at: DateTime.utc_now()
        },
        :upsert
      )

      # Should receive on both topics
      assert_receive {:sse_event, "keyring:upsert", _}
      assert_receive {:sse_event, "keyring:upsert", _}
    end
  end

  describe "broadcast_grant/2" do
    test "broadcasts to owner and recipient" do
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal("did:plc:owner"))
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal("did:plc:recipient"))

      Broadcaster.broadcast_grant(
        %{
          uri: "at://did:plc:owner/app.opake.grant/3gr",
          owner_did: "did:plc:owner",
          recipient_did: "did:plc:recipient",
          document_uri: "at://did:plc:owner/app.opake.document/3doc",
          created_at: "2026-04-10T12:00:00Z"
        },
        :upsert
      )

      assert_receive {:sse_event, "grant:upsert", _}
      assert_receive {:sse_event, "grant:upsert", _}
    end
  end

  describe "broadcast_document_update/2" do
    test "routes upsert to workspace topic when keyring_uri is present" do
      keyring = "at://did:plc:bctest_doccast_routed/app.opake.keyring/3kr"
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.workspace(keyring))

      Broadcaster.broadcast_document_update(
        %{
          uri: "at://did:plc:bctest_doccast_ed/app.opake.documentUpdate/3upd",
          document_uri: "at://did:plc:bctest_doccast_routed/app.opake.document/3doc",
          author_did: "did:plc:bctest_doccast_ed",
          keyring_uri: keyring
        },
        :upsert
      )

      assert_receive {:sse_event, "document_update:upsert", payload}
      assert payload.document_uri == "at://did:plc:bctest_doccast_routed/app.opake.document/3doc"
      assert payload.keyring_uri == keyring
    end

    test "drops upsert silently when keyring_uri is absent" do
      author = "did:plc:bctest_doccast_dropped"
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal(author))

      Broadcaster.broadcast_document_update(
        %{
          uri: "at://did:plc:bctest_doccast_dropped/app.opake.documentUpdate/3upd",
          document_uri: "at://did:plc:bctest_doccast_dropped/app.opake.document/3doc",
          author_did: author
        },
        :upsert
      )

      # No broadcast reaches any topic — the event is dropped at the
      # broadcaster, logged at debug. The DB row (written by the
      # indexer) is the backstop: the owner's next
      # `sync_workspace_by_uri` call will pick it up from the
      # persistent proposal store.
      refute_receive {:sse_event, "document_update:upsert", _}, 50
    end

    test "drops delete silently when keyring_uri is absent" do
      author = "did:plc:bctest_doccast_delete"
      Phoenix.PubSub.subscribe(@pubsub, OpakeIndexer.SSE.Topics.personal(author))

      Broadcaster.broadcast_document_update(
        %{
          uri: "at://did:plc:bctest_doccast_delete/app.opake.documentUpdate/3upd",
          author_did: author
        },
        :delete
      )

      refute_receive {:sse_event, "document_update:delete", _}, 50
    end
  end
end
