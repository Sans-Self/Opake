defmodule OpakeAppview.BackfillTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Backfill
  alias OpakeAppview.Queries.{DirectoryQueries, GrantQueries, KeyringQueries}

  @did "did:plc:owner"

  describe "index_record/4 for keyrings" do
    test "indexes a keyring with members" do
      now = DateTime.utc_now()
      keyring_uri = "at://#{@did}/app.opake.keyring/3kr"

      record = %{
        "uri" => keyring_uri,
        "value" => %{
          "members" => [
            %{
              "wrappedKey" => %{"did" => "did:plc:alice", "$bytes" => "AAAA"},
              "role" => "manager"
            },
            %{
              "wrappedKey" => %{"did" => "did:plc:bob", "$bytes" => "BBBB"},
              "role" => "editor"
            }
          ],
          "rotation" => 0,
          "encryptedMetadata" => %{"ciphertext" => "META", "nonce" => "NONCE"},
          "createdAt" => "2026-04-01T00:00:00Z"
        }
      }

      Backfill.index_record(@did, "app.opake.keyring", record, now)

      {alice, _} = KeyringQueries.list_keyrings_for_member("did:plc:alice")
      assert length(alice) == 1
      assert hd(alice).uri == keyring_uri

      {bob, _} = KeyringQueries.list_keyrings_for_member("did:plc:bob")
      assert length(bob) == 1
    end
  end

  describe "index_record/4 for grants" do
    test "indexes a sharing grant" do
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{@did}/app.opake.grant/3gr",
        "value" => %{
          "recipient" => "did:plc:recipient",
          "document" => "at://#{@did}/app.opake.document/3doc",
          "createdAt" => "2026-04-01T00:00:00Z"
        }
      }

      Backfill.index_record(@did, "app.opake.grant", record, now)

      {grants, _} = GrantQueries.list_inbox("did:plc:recipient")
      assert length(grants) == 1
      assert hd(grants).owner_did == @did
    end

    test "skips grant with missing fields" do
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{@did}/app.opake.grant/3gr",
        "value" => %{"recipient" => "did:plc:recipient"}
      }

      assert :ok == Backfill.index_record(@did, "app.opake.grant", record, now)
    end
  end

  describe "index_record/4 for directories" do
    test "indexes a cabinet directory (directKeyWrapping)" do
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{@did}/app.opake.directory/3dir",
        "value" => %{
          "entries" => ["at://#{@did}/app.opake.document/3a"],
          "encryptedMetadata" => %{"ciphertext" => "AAAA", "nonce" => "BBBB"},
          "keyWrapping" => %{
            "$type" => "app.opake.directory#directKeyWrapping",
            "wrappedKey" => %{"$bytes" => "DDDD"}
          }
        }
      }

      Backfill.index_record(@did, "app.opake.directory", record, now)

      {dirs, _docs} = DirectoryQueries.cabinet_tree(@did)
      assert length(dirs) == 1
      dir = hd(dirs)
      assert dir.directory_uri == "at://#{@did}/app.opake.directory/3dir"
      assert dir.keyring_uri == nil
      assert dir.entries == ["at://#{@did}/app.opake.document/3a"]
    end

    test "indexes a workspace directory (keyringKeyWrapping)" do
      now = DateTime.utc_now()
      keyring = "at://#{@did}/app.opake.keyring/3kr"

      record = %{
        "uri" => "at://#{@did}/app.opake.directory/3dir",
        "value" => %{
          "entries" => [],
          "keyWrapping" => %{
            "$type" => "app.opake.directory#keyringKeyWrapping",
            "keyringRef" => %{"keyring" => keyring, "rotation" => 0},
            "wrappedKey" => %{"$bytes" => "AAAA"}
          }
        }
      }

      Backfill.index_record(@did, "app.opake.directory", record, now)

      {dirs, _docs} = DirectoryQueries.workspace_tree(keyring)
      assert length(dirs) == 1
      assert hd(dirs).keyring_uri == keyring
    end
  end

  describe "index_record/4 for documents" do
    test "indexes a cabinet document (directEncryption)" do
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{@did}/app.opake.document/3doc",
        "value" => %{
          "encryptedMetadata" => %{"ciphertext" => "CCCC"},
          "encryption" => %{
            "$type" => "app.opake.document#directEncryption",
            "wrappedKey" => %{"$bytes" => "EEEE"},
            "nonce" => "FFFF"
          },
          "blob" => %{"$type" => "blob", "size" => 1024}
        }
      }

      Backfill.index_record(@did, "app.opake.document", record, now)

      {_dirs, docs} = DirectoryQueries.cabinet_tree(@did)
      assert length(docs) == 1
      assert hd(docs).document_uri == "at://#{@did}/app.opake.document/3doc"
    end

    test "indexes a workspace document (keyringEncryption)" do
      now = DateTime.utc_now()
      keyring = "at://#{@did}/app.opake.keyring/3kr"

      record = %{
        "uri" => "at://#{@did}/app.opake.document/3doc",
        "value" => %{
          "encryptedMetadata" => %{"ciphertext" => "CCCC"},
          "encryption" => %{
            "$type" => "app.opake.document#keyringEncryption",
            "keyringRef" => %{"keyring" => keyring, "rotation" => 3},
            "nonce" => "AABB"
          },
          "blob" => %{"$type" => "blob", "size" => 2048}
        }
      }

      Backfill.index_record(@did, "app.opake.document", record, now)

      {_dirs, docs} = DirectoryQueries.workspace_tree(keyring)
      assert length(docs) == 1
      assert hd(docs).rotation == 3
    end
  end

  test "index_record ignores unknown collections" do
    assert :ok == Backfill.index_record(@did, "app.opake.unknown", %{}, DateTime.utc_now())
  end
end
