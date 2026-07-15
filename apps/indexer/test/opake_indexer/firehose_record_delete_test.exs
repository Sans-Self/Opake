defmodule OpakeIndexer.FirehoseRecordDeleteTest do
  @moduledoc """
  Pipeline coverage for non-keyring record deletes (grant / directory /
  document): Jetstream delete JSON in, tombstone + fan-out SSE broadcast out.

  Regression: `broadcast_record_delete` used to hand the Ecto record struct
  straight to `fan_out`, whose grant/directory/document clauses bracket-index
  their argument (`attrs[:record_jsonb]`, `attrs[:workspace_id]`,
  `attrs[:author_did]`). Structs don't implement Access, so the lookup raised
  `UndefinedFunctionError`, the module's `rescue` swallowed it, and the delete
  broadcast was silently dropped — a revoked grant never left the recipient's
  live inbox, and cabinet document/directory deletes never reached open
  clients. These tests drive the real dispatch and pin that each delete now
  fans out to the correct topic(s).
  """

  use OpakeIndexer.DataCase, async: false

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Queries.RecordQueries
  alias OpakeIndexer.SSE.Topics

  @owner "did:plc:owner"
  @recipient "did:plc:recipient"
  @workspace "at://#{@owner}/at.opake.keyring/genesis"

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

  defp now, do: DateTime.utc_now()

  defp seed(uri, collection, opts) do
    {:ok, _} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: collection,
        author_did: Keyword.get(opts, :author_did, @owner),
        workspace_id: Keyword.get(opts, :workspace_id),
        is_workspace_root: Keyword.get(opts, :is_workspace_root, false),
        cid: "bafy-#{uri}",
        indexed_at: now(),
        record_jsonb: Keyword.get(opts, :record_jsonb, %{})
      })
  end

  describe "grant delete fan-out" do
    # spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
    test "grant delete reaches the recipient's personal topic" do
      grant = "at://#{@owner}/at.opake.grant/g1"
      seed(grant, "at.opake.grant", record_jsonb: %{"recipient" => @recipient})

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.personal(@recipient))
      process_delete(grant)

      assert_receive {:sse_event, "at.opake.grant:delete", %{uri: ^grant}}
      assert %{deleted_at: %DateTime{}} = RecordQueries.lookup(grant)
    end

    # spec:sharing-grants § The recipient discovers shares through the indexer, not by polling PDSes
    test "grant delete also reaches the owner's personal topic" do
      grant = "at://#{@owner}/at.opake.grant/g2"
      seed(grant, "at.opake.grant", record_jsonb: %{"recipient" => @recipient})

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.personal(@owner))
      process_delete(grant)

      assert_receive {:sse_event, "at.opake.grant:delete", %{uri: ^grant}}
    end
  end

  describe "directory / document delete fan-out" do
    test "workspace-scoped directory delete reaches the workspace topic" do
      dir = "at://#{@owner}/at.opake.directory/d1"
      seed(dir, "at.opake.directory", workspace_id: @workspace)

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.workspace(@workspace))
      process_delete(dir)

      assert_receive {:sse_event, "at.opake.directory:delete", %{uri: ^dir}}
    end

    test "cabinet document delete (no workspace) reaches the author's personal topic" do
      doc = "at://#{@owner}/at.opake.document/doc1"
      seed(doc, "at.opake.document", workspace_id: nil, author_did: @owner)

      :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.personal(@owner))
      process_delete(doc)

      assert_receive {:sse_event, "at.opake.document:delete", %{uri: ^doc}}
    end
  end
end
