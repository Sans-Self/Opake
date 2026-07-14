defmodule OpakeIndexer.SSE.BroadcasterTest do
  @moduledoc """
  Unit coverage for `SSE.Broadcaster`'s topic fan-out, independent of the
  firehose pipeline. Pipeline-level delete-fanout coverage (the
  struct-vs-map regression from 8c8bdfd) already lives in
  `FirehoseRecordDeleteTest` / `FirehoseKeyringDeleteTest`; this file drives
  the broadcaster functions directly to pin routing per collection.
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.SSE.{Broadcaster, Topics}
  alias OpakeIndexer.Schemas.Record, as: RecordSchema

  @alice "did:plc:alice"
  @bob "did:plc:bob"
  @carol "did:plc:carol"

  defp subscribe(topic), do: :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, topic)

  describe "broadcast_record_upsert — keyring" do
    test "fans out to the workspace topic and every member's personal topic" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"

      attrs = %{
        collection: "app.opake.keyring",
        workspace_id: ws,
        author_did: @alice,
        record_jsonb: %{
          "members" => [
            %{"wrappedKey" => %{"did" => @alice}, "role" => "manager"},
            %{"wrappedKey" => %{"did" => @bob}, "role" => "editor"}
          ]
        }
      }

      envelope = %{record: attrs.record_jsonb, indexedAt: "2026-01-01T00:00:00Z"}

      subscribe(Topics.workspace(ws))
      subscribe(Topics.personal(@alice))
      subscribe(Topics.personal(@bob))

      Broadcaster.broadcast_record_upsert(attrs, envelope)

      assert_receive {:sse_event, "app.opake.keyring:upsert", ^envelope}
      assert_receive {:sse_event, "app.opake.keyring:upsert", ^envelope}
      assert_receive {:sse_event, "app.opake.keyring:upsert", ^envelope}
    end

    test "a member not on the head list never receives the event" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"

      attrs = %{
        collection: "app.opake.keyring",
        workspace_id: ws,
        author_did: @alice,
        record_jsonb: %{"members" => [%{"wrappedKey" => %{"did" => @alice}, "role" => "manager"}]}
      }

      subscribe(Topics.personal(@carol))
      Broadcaster.broadcast_record_upsert(attrs, %{record: attrs.record_jsonb, indexedAt: "x"})

      refute_receive {:sse_event, _, _}
    end
  end

  describe "broadcast_record_upsert — grant" do
    test "fans out to both recipient and author personal topics, not the workspace topic" do
      attrs = %{
        collection: "app.opake.grant",
        workspace_id: nil,
        author_did: @alice,
        record_jsonb: %{"recipient" => @bob}
      }

      envelope = %{record: attrs.record_jsonb, indexedAt: "2026-01-01T00:00:00Z"}

      subscribe(Topics.personal(@alice))
      subscribe(Topics.personal(@bob))

      Broadcaster.broadcast_record_upsert(attrs, envelope)

      assert_receive {:sse_event, "app.opake.grant:upsert", ^envelope}
      assert_receive {:sse_event, "app.opake.grant:upsert", ^envelope}
    end
  end

  describe "broadcast_record_upsert — directory / document" do
    test "workspace-scoped record fans out only to the workspace topic" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"

      attrs = %{
        collection: "app.opake.directory",
        workspace_id: ws,
        author_did: @alice,
        record_jsonb: %{}
      }

      envelope = %{record: %{}, indexedAt: "x"}

      subscribe(Topics.workspace(ws))
      subscribe(Topics.personal(@alice))

      Broadcaster.broadcast_record_upsert(attrs, envelope)

      assert_receive {:sse_event, "app.opake.directory:upsert", ^envelope}
      refute_receive {:sse_event, _, _}
    end

    test "cabinet record (no workspace_id) fans out only to the author's personal topic" do
      attrs = %{
        collection: "app.opake.document",
        workspace_id: nil,
        author_did: @alice,
        record_jsonb: %{}
      }

      envelope = %{record: %{}, indexedAt: "x"}

      subscribe(Topics.personal(@alice))

      Broadcaster.broadcast_record_upsert(attrs, envelope)

      assert_receive {:sse_event, "app.opake.document:upsert", ^envelope}
    end
  end

  describe "broadcast_record_delete" do
    # Regression for 8c8bdfd: fan_out's grant/directory/document clauses
    # bracket-index their argument. Handing them the Ecto struct directly
    # (no Access implementation) used to raise inside the `rescue`, silently
    # dropping the broadcast. This pins that a struct input still fans out.
    test "bug__struct_input_reaches_recipient_topic delivers a grant delete to the recipient" do
      grant = %RecordSchema{
        uri: "at://#{@alice}/app.opake.grant/g1",
        collection: "app.opake.grant",
        author_did: @alice,
        workspace_id: nil,
        record_jsonb: %{"recipient" => @bob}
      }

      subscribe(Topics.personal(@bob))

      Broadcaster.broadcast_record_delete(grant)

      assert_receive {:sse_event, "app.opake.grant:delete", %{uri: uri}}
      assert uri == grant.uri
    end

    test "workspace-scoped directory delete reaches the workspace topic" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"

      directory = %RecordSchema{
        uri: "at://#{@alice}/app.opake.directory/d1",
        collection: "app.opake.directory",
        author_did: @alice,
        workspace_id: ws,
        record_jsonb: %{}
      }

      subscribe(Topics.workspace(ws))
      Broadcaster.broadcast_record_delete(directory)

      assert_receive {:sse_event, "app.opake.directory:delete", %{uri: uri}}
      assert uri == directory.uri
    end
  end

  describe "broadcast_keyring_delete" do
    test "fans out to the workspace topic and every deleted member's personal topic with the outcome" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"

      record = %RecordSchema{
        uri: ws,
        collection: "app.opake.keyring",
        author_did: @alice,
        workspace_id: ws,
        record_jsonb: %{
          "members" => [
            %{"wrappedKey" => %{"did" => @alice}, "role" => "manager"},
            %{"wrappedKey" => %{"did" => @bob}, "role" => "editor"}
          ]
        }
      }

      subscribe(Topics.workspace(ws))
      subscribe(Topics.personal(@bob))

      Broadcaster.broadcast_keyring_delete(record, "torn_down")

      assert_receive {:sse_event, "app.opake.keyring:delete",
                      %{uri: ^ws, workspace_id: ^ws, outcome: "torn_down"}}

      assert_receive {:sse_event, "app.opake.keyring:delete",
                      %{uri: ^ws, workspace_id: ^ws, outcome: "torn_down"}}
    end

    test "an orphan row (nil workspace_id) uses its own uri as workspace identity" do
      orphan = "at://#{@alice}/app.opake.keyring/orphan"

      record = %RecordSchema{
        uri: orphan,
        collection: "app.opake.keyring",
        author_did: @alice,
        workspace_id: nil,
        record_jsonb: %{"members" => []}
      }

      subscribe(Topics.workspace(orphan))
      Broadcaster.broadcast_keyring_delete(record, "unchanged")

      assert_receive {:sse_event, "app.opake.keyring:delete", %{workspace_id: ^orphan}}
    end
  end

  describe "broadcast_chain_forked" do
    test "broadcasts to the workspace topic derived from the payload's workspace_id" do
      ws = "at://#{@alice}/app.opake.keyring/genesis"
      payload = %{workspace_id: ws, scope: "keyring", your_uri: "a", fork_point_uri: "b"}

      subscribe(Topics.workspace(ws))
      Broadcaster.broadcast_chain_forked(payload)

      assert_receive {:sse_event, "chain:forked", ^payload}
    end

    test "does nothing when the payload carries no workspace_id" do
      subscribe(Topics.workspace("nonexistent"))
      Broadcaster.broadcast_chain_forked(%{scope: "keyring"})

      refute_receive {:sse_event, _, _}
    end
  end
end
