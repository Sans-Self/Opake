defmodule OpakeAppview.IndexerTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Indexer
  alias OpakeAppview.Queries.{GrantQueries, KeyringQueries, DirectoryQueries}

  # Grant helpers

  defp grant_create_json do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.grant",
        "rkey" => "3abc",
        "record" => %{
          "recipient" => "did:plc:recipient",
          "document" => "at://did:plc:owner/app.opake.document/3xyz",
          "createdAt" => "2026-03-01T12:00:00Z"
        }
      }
    })
  end

  defp grant_delete_json do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "delete",
        "collection" => "app.opake.grant",
        "rkey" => "3abc"
      }
    })
  end

  # Keyring helpers

  defp keyring_create_json(members) do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.keyring",
        "rkey" => "3def",
        "record" => %{
          "members" =>
            Enum.map(
              members,
              &%{
                "wrappedKey" => %{
                  "did" => &1,
                  "ciphertext" => %{"$bytes" => "AAAA"},
                  "algo" => "x25519-hkdf-a256kw"
                },
                "role" => "manager"
              }
            )
        }
      }
    })
  end

  defp keyring_update_json(members) do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "update",
        "collection" => "app.opake.keyring",
        "rkey" => "3def",
        "record" => %{
          "members" =>
            Enum.map(
              members,
              &%{
                "wrappedKey" => %{
                  "did" => &1,
                  "ciphertext" => %{"$bytes" => "AAAA"},
                  "algo" => "x25519-hkdf-a256kw"
                },
                "role" => "manager"
              }
            )
        }
      }
    })
  end

  defp keyring_delete_json do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "delete",
        "collection" => "app.opake.keyring",
        "rkey" => "3def"
      }
    })
  end

  # Directory helpers

  defp directory_create_json(key_wrapping) do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.directory",
        "rkey" => "3dir",
        "record" => %{
          "entries" => ["at://did:plc:owner/app.opake.document/3xyz"],
          "encryptedMetadata" => %{"ciphertext" => "AAAA", "nonce" => "BBBB"},
          "keyWrapping" => key_wrapping
        }
      }
    })
  end

  defp directory_delete_json do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "delete",
        "collection" => "app.opake.directory",
        "rkey" => "3dir"
      }
    })
  end

  # Document helpers

  defp document_create_json(encryption) do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.document",
        "rkey" => "3doc",
        "record" => %{
          "encryptedMetadata" => %{"ciphertext" => "CCCC", "nonce" => "DDDD"},
          "encryption" => encryption,
          "blob" => %{
            "$type" => "blob",
            "ref" => %{"$link" => "bafyblob"},
            "mimeType" => "application/octet-stream",
            "size" => 1024
          }
        }
      }
    })
  end

  defp document_delete_json do
    Jason.encode!(%{
      "did" => "did:plc:owner",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "delete",
        "collection" => "app.opake.document",
        "rkey" => "3doc"
      }
    })
  end

  # Grant pipeline tests

  test "indexes grant create" do
    Indexer.process_message(grant_create_json(), 0)

    {grants, _} = GrantQueries.list_inbox("did:plc:recipient")
    assert length(grants) == 1
    assert hd(grants).owner_did == "did:plc:owner"
  end

  test "indexes grant delete" do
    Indexer.process_message(grant_create_json(), 0)
    Indexer.process_message(grant_delete_json(), 1)

    {grants, _} = GrantQueries.list_inbox("did:plc:recipient")
    assert grants == []
  end

  # Keyring pipeline tests

  test "indexes keyring create" do
    Indexer.process_message(keyring_create_json(["did:plc:alice", "did:plc:bob"]), 0)

    {alice, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
    assert length(alice) == 1

    {bob, _} = KeyringQueries.list_keyrings_for_member("did:plc:bob")
    assert length(bob) == 1
  end

  test "indexes keyring update replaces members" do
    Indexer.process_message(keyring_create_json(["did:plc:alice", "did:plc:bob"]), 0)
    Indexer.process_message(keyring_update_json(["did:plc:alice", "did:plc:charlie"]), 1)

    {bob, _} = KeyringQueries.list_keyrings_for_member("did:plc:bob")
    assert bob == []

    {charlie, _} = KeyringQueries.list_keyrings_for_member("did:plc:charlie")
    assert length(charlie) == 1
  end

  test "indexes keyring delete" do
    Indexer.process_message(keyring_create_json(["did:plc:alice"]), 0)
    Indexer.process_message(keyring_delete_json(), 1)

    {alice, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
    assert alice == []
  end

  # Directory pipeline tests — cabinet (directKeyWrapping)

  test "indexes cabinet directory (directKeyWrapping)" do
    key_wrapping = %{
      "$type" => "app.opake.directory#directKeyWrapping",
      "wrappedKey" => %{"$bytes" => "DDDD"}
    }

    Indexer.process_message(directory_create_json(key_wrapping), 0)

    {dirs, _docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(dirs) == 1
    dir = hd(dirs)
    assert dir.directory_uri == "at://did:plc:owner/app.opake.directory/3dir"
    assert dir.keyring_uri == nil
    assert dir.entries == ["at://did:plc:owner/app.opake.document/3xyz"]
    assert dir.encrypted_metadata == %{"ciphertext" => "AAAA", "nonce" => "BBBB"}
  end

  # Directory pipeline tests — workspace (keyringKeyWrapping)

  test "indexes workspace directory (keyringKeyWrapping)" do
    key_wrapping = %{
      "$type" => "app.opake.directory#keyringKeyWrapping",
      "keyringRef" => %{
        "keyring" => "at://did:plc:owner/app.opake.keyring/3def",
        "rotation" => 0
      },
      "wrappedKey" => %{"$bytes" => "AAAA"}
    }

    Indexer.process_message(directory_create_json(key_wrapping), 0)

    {dirs, _docs} = DirectoryQueries.workspace_tree("at://did:plc:owner/app.opake.keyring/3def")
    assert length(dirs) == 1
    dir = hd(dirs)
    assert dir.keyring_uri == "at://did:plc:owner/app.opake.keyring/3def"
  end

  # Directory soft-delete

  test "soft-deletes directory on delete event" do
    key_wrapping = %{
      "$type" => "app.opake.directory#directKeyWrapping",
      "wrappedKey" => %{"$bytes" => "DDDD"}
    }

    Indexer.process_message(directory_create_json(key_wrapping), 0)
    Indexer.process_message(directory_delete_json(), 1)

    # Should not appear in cabinet tree
    {dirs, _docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert dirs == []

    # But should appear in changes_since (with deleted_at set)
    past = DateTime.add(DateTime.utc_now(), -60, :second)
    {changed_dirs, _docs} = DirectoryQueries.cabinet_changes_since("did:plc:owner", past)
    assert length(changed_dirs) == 1
    assert hd(changed_dirs).deleted_at != nil
  end

  # Document pipeline tests — cabinet (directEncryption)

  test "indexes cabinet document (directEncryption)" do
    encryption = %{
      "$type" => "app.opake.document#directEncryption",
      "wrappedKey" => %{"$bytes" => "EEEE"},
      "nonce" => "FFFF"
    }

    Indexer.process_message(document_create_json(encryption), 0)

    {_dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert length(docs) == 1
    doc = hd(docs)
    assert doc.document_uri == "at://did:plc:owner/app.opake.document/3doc"
    assert doc.keyring_uri == nil
    assert doc.rotation == nil
    assert doc.encrypted_metadata == %{"ciphertext" => "CCCC", "nonce" => "DDDD"}
    assert doc.encryption == encryption
  end

  # Document pipeline tests — workspace (keyringEncryption)

  test "indexes workspace document (keyringEncryption)" do
    encryption = %{
      "$type" => "app.opake.document#keyringEncryption",
      "keyringRef" => %{
        "keyring" => "at://did:plc:owner/app.opake.keyring/3def",
        "rotation" => 2
      },
      "nonce" => "AABBCC"
    }

    Indexer.process_message(document_create_json(encryption), 0)

    {_dirs, docs} = DirectoryQueries.workspace_tree("at://did:plc:owner/app.opake.keyring/3def")
    assert length(docs) == 1
    doc = hd(docs)
    assert doc.keyring_uri == "at://did:plc:owner/app.opake.keyring/3def"
    assert doc.rotation == 2
  end

  # Document soft-delete

  test "soft-deletes document on delete event" do
    encryption = %{
      "$type" => "app.opake.document#directEncryption",
      "wrappedKey" => %{"$bytes" => "EEEE"},
      "nonce" => "FFFF"
    }

    Indexer.process_message(document_create_json(encryption), 0)
    Indexer.process_message(document_delete_json(), 1)

    # Should not appear in cabinet tree
    {_dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:owner")
    assert docs == []

    # But should appear in changes_since
    past = DateTime.add(DateTime.utc_now(), -60, :second)
    {_dirs, changed_docs} = DirectoryQueries.cabinet_changes_since("did:plc:owner", past)
    assert length(changed_docs) == 1
    assert hd(changed_docs).deleted_at != nil
  end
end
