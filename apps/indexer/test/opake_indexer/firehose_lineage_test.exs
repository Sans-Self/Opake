defmodule OpakeIndexer.FirehoseLineageTest do
  @moduledoc """
  Pipeline coverage for the lineage never-flips gate on document supersedes
  (`spec:lineage § Lineage never flips across a supersede`).

  Drives real Jetstream create commits through `Firehose.process_message/2`
  and asserts the observable consequence of the rule: a supersede whose
  declared `lineage` matches its predecessor's anchor lands in the `records`
  table; one that flips the lineage never does. A genesis document (no
  `supersedes`, no `lineage`) always lands.

  Documents carry a `workspaceId` here so they route like real workspace
  documents; the lineage check itself is workspace-independent — it compares
  the supersede's declared `lineage` against the predecessor's anchor
  (`predecessor.lineage || predecessor URI`).
  """

  use OpakeIndexer.DataCase, async: false

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Queries.RecordQueries

  @author "did:plc:author"
  @workspace_id "at://#{@author}/at.opake.keyring/genesis"

  # -- Record fixtures ------------------------------------------------

  defp bytes, do: %{"$bytes" => "AA"}

  defp wrapped_key do
    %{"did" => "did:plc:x", "ciphertext" => bytes(), "algo" => "x25519-mlkem768-hkdf-a256kw-v2"}
  end

  defp document(overrides) do
    Map.merge(
      %{
        "opakeVersion" => 1,
        "blob" => %{
          "$type" => "blob",
          "ref" => %{"$link" => "bafyblob"},
          "mimeType" => "application/octet-stream",
          "size" => 10
        },
        "encryption" => %{
          "$type" => "at.opake.document#directEncryption",
          "envelope" => %{"algo" => "aes-256-gcm", "nonce" => bytes(), "keys" => [wrapped_key()]}
        },
        "encryptedMetadata" => %{"ciphertext" => bytes(), "nonce" => bytes()},
        "workspaceId" => @workspace_id,
        "createdAt" => "2026-01-01T00:00:00Z"
      },
      overrides
    )
  end

  defp create_json(rkey, record) do
    Jason.encode!(%{
      "kind" => "commit",
      "did" => @author,
      "time_us" => 0,
      "commit" => %{
        "operation" => "create",
        "collection" => "at.opake.document",
        "rkey" => rkey,
        "cid" => "bafy-#{rkey}",
        "record" => record
      }
    })
  end

  defp process(rkey, record), do: Firehose.process_message(create_json(rkey, record), 0)

  defp uri(rkey), do: "at://#{@author}/at.opake.document/#{rkey}"

  describe "document lineage never-flips" do
    # spec:lineage § Lineage never flips across a supersede
    test "genesis (no supersedes, no lineage) is indexed" do
      process("genesis", document(%{}))
      assert RecordQueries.lookup(uri("genesis")) != nil
    end

    # spec:lineage § Lineage never flips across a supersede
    test "supersede carrying the predecessor's URI as lineage is indexed" do
      process("genesis", document(%{}))

      process(
        "head",
        document(%{"supersedes" => uri("genesis"), "lineage" => uri("genesis")})
      )

      assert RecordQueries.lookup(uri("head")) != nil
    end

    # spec:lineage § Lineage never flips across a supersede
    test "supersede up the chain carries the genesis lineage forward and is indexed" do
      process("genesis", document(%{}))

      process(
        "mid",
        document(%{"supersedes" => uri("genesis"), "lineage" => uri("genesis")})
      )

      process(
        "head",
        document(%{"supersedes" => uri("mid"), "lineage" => uri("genesis")})
      )

      assert RecordQueries.lookup(uri("head")) != nil
    end

    # spec:lineage § Lineage never flips across a supersede
    test "supersede declaring a different lineage is rejected — not indexed" do
      process("genesis", document(%{}))

      process(
        "flip",
        document(%{
          "supersedes" => uri("genesis"),
          "lineage" => "at://did:plc:evil/at.opake.document/other"
        })
      )

      assert RecordQueries.lookup(uri("flip")) == nil
    end

    # spec:lineage § Lineage never flips across a supersede
    test "supersede that carries no lineage against an indexed predecessor is rejected" do
      process("genesis", document(%{}))

      process("naked", document(%{"supersedes" => uri("genesis")}))

      assert RecordQueries.lookup(uri("naked")) == nil
    end
  end
end
