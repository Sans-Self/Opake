defmodule OpakeAppview.IndexerTest do
  use OpakeAppview.DataCase, async: true

  alias OpakeAppview.Indexer
  alias OpakeAppview.Queries.{GrantQueries, KeyringQueries}

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
          "members" => Enum.map(members, &%{"did" => &1, "wrappedKey" => %{"$bytes" => "AAAA"}})
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
          "members" => Enum.map(members, &%{"did" => &1, "wrappedKey" => %{"$bytes" => "AAAA"}})
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
end
