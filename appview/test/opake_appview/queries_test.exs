defmodule OpakeAppview.QueriesTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Queries.{CursorQueries, GrantQueries, KeyringQueries}

  defp make_grant(uri, opts \\ []) do
    %{
      uri: uri,
      owner_did: Keyword.get(opts, :owner_did, "did:plc:owner"),
      recipient_did: Keyword.get(opts, :recipient_did, "did:plc:me"),
      document_uri: Keyword.get(opts, :document_uri, "at://did:plc:owner/app.opake.document/3xyz"),
      created_at: Keyword.get(opts, :created_at, "2026-03-01T12:00:00Z"),
      indexed_at: Keyword.get(opts, :indexed_at, DateTime.utc_now())
    }
  end

  # Cursor tests

  test "cursor roundtrip" do
    assert CursorQueries.load_cursor() == nil

    {:ok, _} = CursorQueries.save_cursor(1_709_330_400_000_000)
    cursor = CursorQueries.load_cursor()
    assert cursor.time_us == 1_709_330_400_000_000

    {:ok, _} = CursorQueries.save_cursor(1_709_330_500_000_000)
    cursor = CursorQueries.load_cursor()
    assert cursor.time_us == 1_709_330_500_000_000
  end

  # Grant tests

  test "grant upsert and query" do
    grant = make_grant("at://did:plc:owner/app.opake.grant/3abc")
    {:ok, _} = GrantQueries.upsert_grant(grant)

    {grants, _cursor} = GrantQueries.list_inbox("did:plc:me")
    assert length(grants) == 1
    assert hd(grants).uri == "at://did:plc:owner/app.opake.grant/3abc"
    assert hd(grants).document_uri == "at://did:plc:owner/app.opake.document/3xyz"
  end

  test "grant upsert overwrites" do
    grant = make_grant("at://did:plc:owner/app.opake.grant/3abc")
    {:ok, _} = GrantQueries.upsert_grant(grant)

    updated = %{grant | document_uri: "at://did:plc:owner/app.opake.document/new"}
    {:ok, _} = GrantQueries.upsert_grant(updated)

    {grants, _cursor} = GrantQueries.list_inbox("did:plc:me")
    assert length(grants) == 1
    assert hd(grants).document_uri == "at://did:plc:owner/app.opake.document/new"
  end

  test "grant delete" do
    grant = make_grant("at://did:plc:owner/app.opake.grant/3abc")
    {:ok, _} = GrantQueries.upsert_grant(grant)

    GrantQueries.delete_grant("at://did:plc:owner/app.opake.grant/3abc")

    {grants, _cursor} = GrantQueries.list_inbox("did:plc:me")
    assert grants == []
  end

  test "grant pagination" do
    for i <- 0..4 do
      indexed_at =
        DateTime.new!(~D[2026-03-01], Time.new!(12, 0, i))
        |> DateTime.from_naive!("Etc/UTC")

      grant =
        make_grant(
          "at://did:plc:owner/app.opake.grant/#{i}",
          indexed_at: indexed_at
        )

      {:ok, _} = GrantQueries.upsert_grant(grant)
    end

    {page1, cursor1} = GrantQueries.list_inbox("did:plc:me", limit: 2)
    assert length(page1) == 2
    assert cursor1 != nil

    # Newest first
    assert DateTime.compare(hd(page1).indexed_at, List.last(page1).indexed_at) == :gt

    {page2, _cursor2} = GrantQueries.list_inbox("did:plc:me", limit: 2, cursor: cursor1)
    assert length(page2) == 2

    # Page 2 items are older than page 1's last item
    assert DateTime.compare(hd(page2).indexed_at, List.last(page1).indexed_at) == :lt
  end

  # Keyring tests

  test "keyring upsert and query" do
    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        ["did:plc:alice", "did:plc:bob"]
      )

    {alice_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
    assert length(alice_keyrings) == 1

    {bob_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:bob")
    assert length(bob_keyrings) == 1

    {charlie_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:charlie")
    assert charlie_keyrings == []
  end

  test "keyring update replaces members" do
    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        ["did:plc:alice", "did:plc:bob"]
      )

    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        ["did:plc:alice", "did:plc:charlie"]
      )

    {bob_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:bob")
    assert bob_keyrings == []

    {charlie_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:charlie")
    assert length(charlie_keyrings) == 1
  end

  test "keyring delete" do
    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        ["did:plc:alice"]
      )

    KeyringQueries.delete_keyring("at://did:plc:owner/app.opake.keyring/3def")

    {alice_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
    assert alice_keyrings == []
  end
end
