defmodule OpakeAppview.BackfillTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Backfill
  alias OpakeAppview.Queries.DirectoryQueries

  describe "index_record/4 for directories" do
    test "indexes a cabinet directory (directKeyWrapping)" do
      did = "did:plc:owner"
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{did}/app.opake.directory/3dir",
        "value" => %{
          "entries" => ["at://#{did}/app.opake.document/3a"],
          "encryptedMetadata" => %{"ciphertext" => "AAAA", "nonce" => "BBBB"},
          "keyWrapping" => %{
            "$type" => "app.opake.directory#directKeyWrapping",
            "wrappedKey" => %{"$bytes" => "DDDD"}
          }
        }
      }

      Backfill.index_record(did, "app.opake.directory", record, now)

      {dirs, _docs} = DirectoryQueries.cabinet_tree(did)
      assert length(dirs) == 1
      dir = hd(dirs)
      assert dir.directory_uri == "at://#{did}/app.opake.directory/3dir"
      assert dir.keyring_uri == nil
      assert dir.entries == ["at://#{did}/app.opake.document/3a"]
    end

    test "indexes a workspace directory (keyringKeyWrapping)" do
      did = "did:plc:owner"
      now = DateTime.utc_now()
      keyring = "at://#{did}/app.opake.keyring/3kr"

      record = %{
        "uri" => "at://#{did}/app.opake.directory/3dir",
        "value" => %{
          "entries" => [],
          "keyWrapping" => %{
            "$type" => "app.opake.directory#keyringKeyWrapping",
            "keyringRef" => %{"keyring" => keyring, "rotation" => 0},
            "wrappedKey" => %{"$bytes" => "AAAA"}
          }
        }
      }

      Backfill.index_record(did, "app.opake.directory", record, now)

      {dirs, _docs} = DirectoryQueries.workspace_tree(keyring)
      assert length(dirs) == 1
      assert hd(dirs).keyring_uri == keyring
    end
  end

  describe "index_record/4 for documents" do
    test "indexes a cabinet document (directEncryption)" do
      did = "did:plc:owner"
      now = DateTime.utc_now()

      record = %{
        "uri" => "at://#{did}/app.opake.document/3doc",
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

      Backfill.index_record(did, "app.opake.document", record, now)

      {_dirs, docs} = DirectoryQueries.cabinet_tree(did)
      assert length(docs) == 1
      doc = hd(docs)
      assert doc.document_uri == "at://#{did}/app.opake.document/3doc"
      assert doc.keyring_uri == nil
      assert doc.rotation == nil
    end

    test "indexes a workspace document (keyringEncryption)" do
      did = "did:plc:owner"
      now = DateTime.utc_now()
      keyring = "at://#{did}/app.opake.keyring/3kr"

      record = %{
        "uri" => "at://#{did}/app.opake.document/3doc",
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

      Backfill.index_record(did, "app.opake.document", record, now)

      {_dirs, docs} = DirectoryQueries.workspace_tree(keyring)
      assert length(docs) == 1
      doc = hd(docs)
      assert doc.keyring_uri == keyring
      assert doc.rotation == 3
    end
  end

  test "index_record ignores unknown collections" do
    assert :ok ==
             Backfill.index_record("did:plc:owner", "app.opake.unknown", %{}, DateTime.utc_now())
  end
end
