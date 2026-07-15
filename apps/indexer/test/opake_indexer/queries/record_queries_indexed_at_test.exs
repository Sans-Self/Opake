defmodule OpakeIndexer.Queries.RecordQueriesIndexedAtTest do
  @moduledoc """
  Regression coverage for `indexed_at` first-seen immutability. The upsert
  conflict path must not rewrite `indexed_at` on later events for the same
  URI, so the total order pagination and `changes_since` observe stays
  stable across updates.
  """

  use OpakeIndexer.DataCase, async: true

  alias OpakeIndexer.Queries.RecordQueries

  @recipient "did:plc:recipient"
  @workspace_id "at://did:plc:alice/at.opake.keyring/genesis"

  defp t1, do: ~U[2026-07-12 10:00:00.000000Z]
  defp t_mid, do: ~U[2026-07-12 10:02:30.000000Z]
  defp t2, do: ~U[2026-07-12 10:05:00.000000Z]

  defp put_grant(uri, recipient, indexed_at, extra \\ %{}) do
    {:ok, record} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: RecordQueries.grant_collection(),
        author_did: "did:plc:alice",
        cid: "bafy#{uri}",
        indexed_at: indexed_at,
        record_jsonb: Map.merge(%{"recipient" => recipient}, extra)
      })

    record
  end

  defp put_document(uri, indexed_at, extra \\ %{}) do
    {:ok, record} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: RecordQueries.document_collection(),
        author_did: "did:plc:alice",
        workspace_id: @workspace_id,
        cid: "bafy#{uri}",
        indexed_at: indexed_at,
        record_jsonb: Map.merge(%{"name" => "encrypted"}, extra)
      })

    record
  end

  # spec:indexer-consistency § indexed_at is first-seen
  test "update does not modify indexed_at" do
    uri = "at://did:plc:alice/at.opake.grant/g1"
    put_grant(uri, @recipient, t1())

    put_grant(uri, @recipient, t2(), %{"note" => "later event"})

    assert %{indexed_at: indexed_at} = RecordQueries.lookup(uri)
    assert indexed_at == t1()
  end

  # spec:indexer-consistency § indexed_at is first-seen
  test "update does not reposition a record in an indexed_at-ordered page" do
    older = "at://did:plc:alice/at.opake.grant/older"
    newer = "at://did:plc:alice/at.opake.grant/newer"

    put_grant(older, @recipient, t1())
    put_grant(newer, @recipient, t2())

    {before_update, _} = RecordQueries.inbox(@recipient)
    assert Enum.map(before_update, & &1.uri) == [newer, older]

    # A later event for the older grant would jump it to the head of the
    # page if indexed_at were rewritten.
    put_grant(older, @recipient, t2(), %{"note" => "later event"})

    {after_update, _} = RecordQueries.inbox(@recipient)
    assert Enum.map(after_update, & &1.uri) == [newer, older]
  end

  # spec:indexer-consistency § indexed_at is first-seen
  test "changes_since re-delivers a record updated in place, keyed to its first-seen order" do
    uri = "at://did:plc:alice/at.opake.document/doc1"
    put_document(uri, t1())

    put_document(uri, t2(), %{"name" => "still encrypted"})

    {_directories, documents} = RecordQueries.workspace_changes_since(@workspace_id, t1())

    assert uri in Enum.map(documents, & &1.uri)
    # Pagination order is unmoved: indexed_at is still first-seen.
    assert %{indexed_at: indexed_at} = RecordQueries.lookup(uri)
    assert indexed_at == t1()
  end

  # spec:indexer-consistency § indexed_at is first-seen
  #
  # A delta filter keyed to first-seen `indexed_at` went blind to in-place
  # updates: a record updated after a client's cursor never matched
  # `indexed_at > since`, so catch-up sync silently dropped every mutation.
  # The last-write watermark `updated_at` fixes delivery without moving the
  # record's pagination position.
  test "bug__in_place_update_invisible_to_changes_since" do
    uri = "at://did:plc:alice/at.opake.document/doc1"
    put_document(uri, t1())

    # Client's cursor sits between first-seen and the later in-place update.
    cursor = t_mid()

    put_document(uri, t2(), %{"name" => "renamed, still encrypted"})

    {_directories, documents} = RecordQueries.workspace_changes_since(@workspace_id, cursor)

    assert uri in Enum.map(documents, & &1.uri),
           "in-place update after the cursor must be delivered by changes_since"

    assert %{indexed_at: indexed_at} = RecordQueries.lookup(uri)
    assert indexed_at == t1(), "indexed_at must remain first-seen"
  end
end
