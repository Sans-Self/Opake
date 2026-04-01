defmodule OpakeAppview.QueriesTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Queries.{
    CursorQueries,
    GrantQueries,
    KeyringQueries,
    DirectoryQueries,
    DocumentQueries
  }

  defp make_grant(uri, opts \\ []) do
    %{
      uri: uri,
      owner_did: Keyword.get(opts, :owner_did, "did:plc:owner"),
      recipient_did: Keyword.get(opts, :recipient_did, "did:plc:me"),
      document_uri:
        Keyword.get(opts, :document_uri, "at://did:plc:owner/app.opake.document/3xyz"),
      created_at: Keyword.get(opts, :created_at, "2026-03-01T12:00:00Z"),
      indexed_at: Keyword.get(opts, :indexed_at, DateTime.utc_now())
    }
  end

  defp make_directory(uri, opts \\ []) do
    %{
      directory_uri: uri,
      keyring_uri: Keyword.get(opts, :keyring_uri),
      owner_did: Keyword.get(opts, :owner_did, "did:plc:owner"),
      entries: Keyword.get(opts, :entries, []),
      encrypted_metadata: Keyword.get(opts, :encrypted_metadata),
      key_wrapping: Keyword.get(opts, :key_wrapping),
      deleted_at: Keyword.get(opts, :deleted_at),
      indexed_at: Keyword.get(opts, :indexed_at, DateTime.utc_now())
    }
  end

  defp make_document(uri, opts \\ []) do
    %{
      document_uri: uri,
      keyring_uri: Keyword.get(opts, :keyring_uri),
      owner_did: Keyword.get(opts, :owner_did, "did:plc:owner"),
      rotation: Keyword.get(opts, :rotation),
      encrypted_metadata: Keyword.get(opts, :encrypted_metadata),
      encryption: Keyword.get(opts, :encryption),
      blob_ref: Keyword.get(opts, :blob_ref),
      deleted_at: Keyword.get(opts, :deleted_at),
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
        [%{did: "did:plc:alice", role: "manager"}, %{did: "did:plc:bob", role: "editor"}]
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
        [%{did: "did:plc:alice", role: "manager"}, %{did: "did:plc:bob", role: "editor"}]
      )

    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        [%{did: "did:plc:alice", role: "manager"}, %{did: "did:plc:charlie", role: "viewer"}]
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
        [%{did: "did:plc:alice", role: "manager"}]
      )

    KeyringQueries.delete_keyring("at://did:plc:owner/app.opake.keyring/3def")

    {alice_keyrings, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
    assert alice_keyrings == []
  end

  test "is_member? returns true for members" do
    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        [%{did: "did:plc:alice", role: "manager"}]
      )

    assert KeyringQueries.is_member?("at://did:plc:owner/app.opake.keyring/3def", "did:plc:alice")
    refute KeyringQueries.is_member?("at://did:plc:owner/app.opake.keyring/3def", "did:plc:bob")
  end

  test "all_member_dids returns distinct DIDs" do
    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3a",
        "did:plc:owner",
        [%{did: "did:plc:alice", role: "manager"}, %{did: "did:plc:bob", role: "editor"}]
      )

    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3b",
        "did:plc:owner",
        [%{did: "did:plc:alice", role: "manager"}, %{did: "did:plc:charlie", role: "viewer"}]
      )

    dids = KeyringQueries.all_member_dids() |> Enum.sort()
    assert dids == ["did:plc:alice", "did:plc:bob", "did:plc:charlie"]
  end

  # Directory tests

  test "directory upsert and cabinet tree" do
    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3dir",
          entries: ["at://did:plc:owner/app.opake.document/3xyz"],
          encrypted_metadata: %{"ciphertext" => "AAAA"}
        )
      )

    {dirs, _docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(dirs) == 1
    assert hd(dirs).entries == ["at://did:plc:owner/app.opake.document/3xyz"]
  end

  test "directory upsert overwrites" do
    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3dir",
          entries: ["at://entry1"]
        )
      )

    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3dir",
          entries: ["at://entry1", "at://entry2"]
        )
      )

    {dirs, _} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(dirs) == 1
    assert hd(dirs).entries == ["at://entry1", "at://entry2"]
  end

  test "workspace tree filters by keyring_uri" do
    keyring = "at://did:plc:owner/app.opake.keyring/3kr"

    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3ws",
          keyring_uri: keyring
        )
      )

    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory("at://did:plc:owner/app.opake.directory/3cab")
      )

    {dirs, _docs} = DirectoryQueries.workspace_tree(keyring)
    assert length(dirs) == 1
    assert hd(dirs).keyring_uri == keyring
  end

  test "soft_delete_directory sets deleted_at and clears entries" do
    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3dir",
          entries: ["at://entry1"]
        )
      )

    now = DateTime.utc_now()
    DirectoryQueries.soft_delete_directory("at://did:plc:owner/app.opake.directory/3dir", now)

    # Excluded from tree
    {dirs, _} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert dirs == []

    # Included in changes_since
    past = DateTime.add(now, -60, :second)
    {changed_dirs, _} = DirectoryQueries.cabinet_changes_since("did:plc:owner", past)
    assert length(changed_dirs) == 1
    assert hd(changed_dirs).deleted_at != nil
    assert hd(changed_dirs).entries == []
  end

  test "workspace_changes_since includes modified and deleted records" do
    keyring = "at://did:plc:owner/app.opake.keyring/3kr"
    past = DateTime.add(DateTime.utc_now(), -60, :second)

    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3a",
          keyring_uri: keyring
        )
      )

    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3b",
          keyring_uri: keyring
        )
      )

    {dirs, docs} = DirectoryQueries.workspace_changes_since(keyring, past)
    assert length(dirs) == 1
    assert length(docs) == 1
  end

  # Document tests

  test "document upsert and cabinet tree" do
    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3doc",
          encrypted_metadata: %{"ciphertext" => "CCCC"},
          encryption: %{"$type" => "app.opake.document#directEncryption"},
          blob_ref: %{"$type" => "blob", "size" => 1024}
        )
      )

    {_dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(docs) == 1
    assert hd(docs).encrypted_metadata == %{"ciphertext" => "CCCC"}
  end

  test "document upsert overwrites" do
    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3doc",
          rotation: 0
        )
      )

    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3doc",
          rotation: 1
        )
      )

    {_dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(docs) == 1
    assert hd(docs).rotation == 1
  end

  test "soft_delete_document excludes from tree but includes in changes_since" do
    {:ok, _} =
      DocumentQueries.upsert_document(make_document("at://did:plc:owner/app.opake.document/3doc"))

    now = DateTime.utc_now()
    DocumentQueries.soft_delete_document("at://did:plc:owner/app.opake.document/3doc", now)

    {_dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert docs == []

    past = DateTime.add(now, -60, :second)
    {_dirs, changed_docs} = DirectoryQueries.cabinet_changes_since("did:plc:owner", past)
    assert length(changed_docs) == 1
    assert hd(changed_docs).deleted_at != nil
  end

  test "list_documents returns non-deleted workspace documents" do
    keyring = "at://did:plc:owner/app.opake.keyring/3kr"

    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3a",
          keyring_uri: keyring
        )
      )

    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3b",
          keyring_uri: keyring
        )
      )

    docs = DocumentQueries.list_documents(keyring)
    assert length(docs) == 2

    # Soft-delete one
    DocumentQueries.soft_delete_document(
      "at://did:plc:owner/app.opake.document/3a",
      DateTime.utc_now()
    )

    docs = DocumentQueries.list_documents(keyring)
    assert length(docs) == 1
  end

  # Tombstone purge

  test "purge_tombstones removes old soft-deletes" do
    old_time = DateTime.add(DateTime.utc_now(), -10 * 24 * 3600, :second)

    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3old",
          deleted_at: old_time
        )
      )

    {:ok, _} =
      DocumentQueries.upsert_document(
        make_document(
          "at://did:plc:owner/app.opake.document/3old",
          deleted_at: old_time
        )
      )

    # Recent tombstone — should survive
    {:ok, _} =
      DirectoryQueries.upsert_directory(
        make_directory(
          "at://did:plc:owner/app.opake.directory/3new",
          deleted_at: DateTime.utc_now()
        )
      )

    cutoff = DateTime.add(DateTime.utc_now(), -7 * 24 * 3600, :second)
    {dir_count, doc_count} = DirectoryQueries.purge_tombstones(cutoff)
    assert dir_count == 1
    assert doc_count == 1
  end
end
