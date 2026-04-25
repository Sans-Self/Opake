defmodule OpakeIndexer.Jetstream.EventTest do
  use ExUnit.Case, async: true

  alias OpakeIndexer.Jetstream.Event

  defp grant_event_json(operation) do
    Jason.encode!(%{
      "did" => "did:plc:owner123",
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => operation,
        "collection" => "app.opake.grant",
        "rkey" => "3abc",
        "cid" => "bafyabc",
        "record" => %{
          "recipient" => "did:plc:recipient456",
          "document" => "at://did:plc:owner123/app.opake.document/3xyz",
          "createdAt" => "2026-03-01T12:00:00Z",
          "wrappedKey" => %{
            "$bytes" => "AAAA",
            "algo" => "x25519-hkdf-a256kw"
          },
          "encryptedMetadata" => %{
            "ciphertext" => "AAAA",
            "nonce" => "AAAAAAAAAAAAAAAA"
          }
        }
      }
    })
  end

  defp keyring_event_json(operation) do
    Jason.encode!(%{
      "did" => "did:plc:owner123",
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => operation,
        "collection" => "app.opake.keyring",
        "rkey" => "3def",
        "cid" => "bafydef",
        "record" => %{
          "members" => [
            %{
              "wrappedKey" => %{
                "did" => "did:plc:alice",
                "ciphertext" => %{"$bytes" => "AAAA"},
                "algo" => "x25519-hkdf-a256kw"
              },
              "role" => "manager"
            },
            %{
              "wrappedKey" => %{
                "did" => "did:plc:bob",
                "ciphertext" => %{"$bytes" => "BBBB"},
                "algo" => "x25519-hkdf-a256kw"
              },
              "role" => "editor"
            }
          ],
          "rotation" => 0
        }
      }
    })
  end

  defp directory_event_json(operation, key_wrapping) do
    Jason.encode!(%{
      "did" => "did:plc:owner123",
      "time_us" => 1_709_330_600_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => operation,
        "collection" => "app.opake.directory",
        "rkey" => "3dir",
        "cid" => "bafydir",
        "record" => %{
          "entries" => [
            "at://did:plc:owner123/app.opake.document/3xyz",
            "at://did:plc:owner123/app.opake.directory/3sub"
          ],
          "encryptedMetadata" => %{"ciphertext" => "AAAA", "nonce" => "BBBB"},
          "keyWrapping" => key_wrapping
        }
      }
    })
  end

  defp document_event_json(operation, encryption) do
    Jason.encode!(%{
      "did" => "did:plc:owner123",
      "time_us" => 1_709_330_700_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => operation,
        "collection" => "app.opake.document",
        "rkey" => "3doc",
        "cid" => "bafydoc",
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

  defp delete_event_json(collection, rkey) do
    Jason.encode!(%{
      "did" => "did:plc:owner123",
      "time_us" => 1_709_330_600_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => "delete",
        "collection" => collection,
        "rkey" => rkey,
        "cid" => "bafydel"
      }
    })
  end

  # Grant tests

  test "parses grant create" do
    json = grant_event_json("create")

    assert {1_709_330_400_000_000, "app.opake.grant", {:upsert_grant, attrs}} =
             Event.parse(json)

    assert attrs.uri == "at://did:plc:owner123/app.opake.grant/3abc"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.recipient_did == "did:plc:recipient456"
    assert attrs.document_uri == "at://did:plc:owner123/app.opake.document/3xyz"
    assert attrs.created_at == "2026-03-01T12:00:00Z"
    refute Map.has_key?(attrs, :time_us)
  end

  test "parses grant update" do
    json = grant_event_json("update")
    assert {_time, "app.opake.grant", {:upsert_grant, _attrs}} = Event.parse(json)
  end

  test "parses grant delete" do
    json = delete_event_json("app.opake.grant", "3abc")

    assert {1_709_330_600_000_000, "app.opake.grant", {:delete_grant, %{uri: uri}}} =
             Event.parse(json)

    assert uri == "at://did:plc:owner123/app.opake.grant/3abc"
  end

  # Keyring tests

  test "parses keyring create" do
    json = keyring_event_json("create")

    assert {1_709_330_500_000_000, "app.opake.keyring", {:upsert_keyring, attrs}} =
             Event.parse(json)

    assert attrs.uri == "at://did:plc:owner123/app.opake.keyring/3def"
    assert attrs.owner_did == "did:plc:owner123"

    assert [
             %{did: "did:plc:alice", role: "manager"},
             %{did: "did:plc:bob", role: "editor"}
           ] = Enum.map(attrs.member_entries, &Map.take(&1, [:did, :role]))
  end

  test "parses keyring delete" do
    json = delete_event_json("app.opake.keyring", "3def")

    assert {_time, "app.opake.keyring", {:delete_keyring, %{uri: uri}}} = Event.parse(json)
    assert uri == "at://did:plc:owner123/app.opake.keyring/3def"
  end

  # Directory tests — keyring-wrapped (workspace)

  test "parses directory create with keyringKeyWrapping" do
    key_wrapping = %{
      "$type" => "app.opake.directory#keyringKeyWrapping",
      "keyringRef" => %{
        "keyring" => "at://did:plc:owner123/app.opake.keyring/3def",
        "rotation" => 0
      },
      "wrappedKey" => %{"$bytes" => "AAAA"}
    }

    json = directory_event_json("create", key_wrapping)

    assert {_time, "app.opake.directory", {:upsert_directory, attrs}} = Event.parse(json)
    assert attrs.directory_uri == "at://did:plc:owner123/app.opake.directory/3dir"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.keyring_uri == "at://did:plc:owner123/app.opake.keyring/3def"
    assert length(attrs.entries) == 2
    assert attrs.encrypted_metadata == %{"ciphertext" => "AAAA", "nonce" => "BBBB"}
    assert attrs.key_wrapping == key_wrapping
  end

  # Directory tests — directly wrapped (cabinet)

  test "parses directory create with directKeyWrapping" do
    key_wrapping = %{
      "$type" => "app.opake.directory#directKeyWrapping",
      "wrappedKey" => %{"$bytes" => "DDDD"}
    }

    json = directory_event_json("create", key_wrapping)

    assert {_time, "app.opake.directory", {:upsert_directory, attrs}} = Event.parse(json)
    assert attrs.directory_uri == "at://did:plc:owner123/app.opake.directory/3dir"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.keyring_uri == nil
    assert length(attrs.entries) == 2
    assert attrs.key_wrapping == key_wrapping
  end

  test "parses directory delete" do
    json = delete_event_json("app.opake.directory", "3dir")

    assert {_time, "app.opake.directory", {:delete_directory, %{directory_uri: uri}}} =
             Event.parse(json)
    assert uri == "at://did:plc:owner123/app.opake.directory/3dir"
  end

  test "directory entries filters non-string values" do
    json =
      Jason.encode!(%{
        "did" => "did:plc:owner123",
        "time_us" => 1_709_330_600_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "create",
          "collection" => "app.opake.directory",
          "rkey" => "3dir",
          "record" => %{
            "entries" => ["at://valid", 42, nil, "at://also-valid"],
            "keyWrapping" => %{"$type" => "app.opake.directory#directKeyWrapping"}
          }
        }
      })

    assert {_time, "app.opake.directory", {:upsert_directory, attrs}} = Event.parse(json)
    assert attrs.entries == ["at://valid", "at://also-valid"]
  end

  # Document tests — keyring-encrypted (workspace)

  test "parses document create with keyringEncryption" do
    encryption = %{
      "$type" => "app.opake.document#keyringEncryption",
      "keyringRef" => %{
        "keyring" => "at://did:plc:owner123/app.opake.keyring/3def",
        "rotation" => 2
      },
      "nonce" => "AABBCC"
    }

    json = document_event_json("create", encryption)

    assert {_time, "app.opake.document", {:upsert_document, attrs}} = Event.parse(json)
    assert attrs.document_uri == "at://did:plc:owner123/app.opake.document/3doc"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.keyring_uri == "at://did:plc:owner123/app.opake.keyring/3def"
    assert attrs.rotation == 2
    assert attrs.encrypted_metadata == %{"ciphertext" => "CCCC", "nonce" => "DDDD"}
    assert attrs.encryption == encryption
    assert attrs.blob_ref != nil
  end

  # Document tests — directly encrypted (cabinet)

  test "parses document create with directEncryption" do
    encryption = %{
      "$type" => "app.opake.document#directEncryption",
      "wrappedKey" => %{"$bytes" => "EEEE"},
      "nonce" => "FFFF"
    }

    json = document_event_json("create", encryption)

    assert {_time, "app.opake.document", {:upsert_document, attrs}} = Event.parse(json)
    assert attrs.document_uri == "at://did:plc:owner123/app.opake.document/3doc"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.keyring_uri == nil
    assert attrs.rotation == nil
    assert attrs.encryption == encryption
  end

  test "parses document delete" do
    json = delete_event_json("app.opake.document", "3doc")

    assert {_time, "app.opake.document", {:delete_document, %{document_uri: uri}}} =
             Event.parse(json)
    assert uri == "at://did:plc:owner123/app.opake.document/3doc"
  end

  # General tests

  test "ignores identity events" do
    json = Jason.encode!(%{"kind" => "identity", "did" => "did:plc:test"})
    assert {nil, nil, :ignore} = Event.parse(json)
  end

  test "ignores unknown collections but extracts time_us and collection so cursor + counters advance" do
    json =
      Jason.encode!(%{
        "did" => "did:plc:test",
        "time_us" => 1_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "create",
          "collection" => "app.bsky.feed.post",
          "rkey" => "123",
          "record" => %{}
        }
      })

    assert {1_000_000, "app.bsky.feed.post", :ignore} = Event.parse(json)
  end

  test "ignores malformed json with nil time_us and nil collection" do
    assert {nil, nil, :ignore} = Event.parse("not json at all")
  end

  test "ignores grant with invalid record but reports its collection" do
    json =
      Jason.encode!(%{
        "did" => "did:plc:owner123",
        "time_us" => 1_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "create",
          "collection" => "app.opake.grant",
          "rkey" => "3abc",
          "record" => %{"garbage" => true}
        }
      })

    assert {1_000_000, "app.opake.grant", :ignore} = Event.parse(json)
  end
end
