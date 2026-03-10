defmodule OpakeAppview.Jetstream.EventTest do
  use ExUnit.Case, async: true

  alias OpakeAppview.Jetstream.Event

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
            %{"did" => "did:plc:alice", "wrappedKey" => %{"$bytes" => "AAAA"}},
            %{"did" => "did:plc:bob", "wrappedKey" => %{"$bytes" => "BBBB"}}
          ],
          "rotation" => 0
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

  test "parses grant create" do
    json = grant_event_json("create")

    assert {:upsert_grant, attrs} = Event.parse(json)
    assert attrs.time_us == 1_709_330_400_000_000
    assert attrs.uri == "at://did:plc:owner123/app.opake.grant/3abc"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.recipient_did == "did:plc:recipient456"
    assert attrs.document_uri == "at://did:plc:owner123/app.opake.document/3xyz"
    assert attrs.created_at == "2026-03-01T12:00:00Z"
  end

  test "parses grant update" do
    json = grant_event_json("update")
    assert {:upsert_grant, _attrs} = Event.parse(json)
  end

  test "parses grant delete" do
    json = delete_event_json("app.opake.grant", "3abc")

    assert {:delete_grant, %{uri: uri}} = Event.parse(json)
    assert uri == "at://did:plc:owner123/app.opake.grant/3abc"
  end

  test "parses keyring create" do
    json = keyring_event_json("create")

    assert {:upsert_keyring, attrs} = Event.parse(json)
    assert attrs.time_us == 1_709_330_500_000_000
    assert attrs.uri == "at://did:plc:owner123/app.opake.keyring/3def"
    assert attrs.owner_did == "did:plc:owner123"
    assert attrs.member_dids == ["did:plc:alice", "did:plc:bob"]
  end

  test "parses keyring delete" do
    json = delete_event_json("app.opake.keyring", "3def")

    assert {:delete_keyring, %{uri: uri}} = Event.parse(json)
    assert uri == "at://did:plc:owner123/app.opake.keyring/3def"
  end

  test "ignores identity events" do
    json = Jason.encode!(%{"kind" => "identity", "did" => "did:plc:test"})
    assert :ignore = Event.parse(json)
  end

  test "ignores unknown collections" do
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

    assert :ignore = Event.parse(json)
  end

  test "ignores malformed json" do
    assert :ignore = Event.parse("not json at all")
  end

  test "ignores grant with invalid record" do
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

    assert :ignore = Event.parse(json)
  end
end
