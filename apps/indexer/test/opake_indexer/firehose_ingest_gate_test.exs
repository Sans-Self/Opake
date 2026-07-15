defmodule OpakeIndexer.FirehoseIngestGateTest do
  @moduledoc """
  Pipeline coverage for the ingest gate (D6 / `record-validity` § indexer
  validates structure for all versions and vocabulary for known versions).

  Drives real Jetstream create commits through `Firehose.process_message/2` and
  asserts the two observable consequences of a refusal: the record never lands
  in the `records` table (absent from every snapshot query) and no envelope is
  broadcast (absent from the stream). A well-formed future-version record does
  the opposite — indexed and relayed verbatim.

  Cabinet documents (no `workspaceId`) are used throughout: their upsert fans
  out to the author's personal topic, the simplest observable stream surface.
  """

  use OpakeIndexer.DataCase, async: false

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Queries.RecordQueries
  alias OpakeIndexer.SSE.Topics

  @author "did:plc:author"

  # -- Record fixtures ------------------------------------------------

  defp bytes, do: %{"$bytes" => "AA"}

  defp wrapped_key(algo \\ "x25519-mlkem768-hkdf-a256kw-v2") do
    %{"did" => "did:plc:x", "ciphertext" => bytes(), "algo" => algo}
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

  setup do
    :ok = Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, Topics.personal(@author))
    :ok
  end

  describe "refused records are absent from snapshot and stream" do
    test "malformed record (missing encryption) is not indexed and not broadcast" do
      rkey = "malformed"
      process(rkey, document(%{}) |> Map.delete("encryption"))

      assert RecordQueries.lookup(uri(rkey)) == nil
      refute_receive {:sse_event, "at.opake.document:upsert", _}, 150
    end

    test "vocabulary-violating known-version record is refused" do
      rkey = "badvocab"

      bad =
        document(%{
          "encryption" => %{
            "$type" => "at.opake.document#directEncryption",
            "envelope" => %{"algo" => "des", "nonce" => bytes(), "keys" => [wrapped_key()]}
          }
        })

      process(rkey, bad)

      assert RecordQueries.lookup(uri(rkey)) == nil
      refute_receive {:sse_event, "at.opake.document:upsert", _}, 150
    end

    test "claimed-future malformed record is refused like any other malformed record" do
      rkey = "futuregarbage"
      garbage = document(%{"opakeVersion" => 999}) |> Map.delete("encryption")

      process(rkey, garbage)

      assert RecordQueries.lookup(uri(rkey)) == nil
      refute_receive {:sse_event, "at.opake.document:upsert", _}, 150
    end
  end

  describe "well-formed records enter snapshot and stream" do
    test "well-formed known-version record is indexed and broadcast" do
      rkey = "good"
      process(rkey, document(%{}))

      assert %{uri: _} = RecordQueries.lookup(uri(rkey))
      assert_receive {:sse_event, "at.opake.document:upsert", %{record: %{"opakeVersion" => 1}}}
    end

    test "well-formed future-version record is indexed and relayed verbatim" do
      rkey = "future"

      future =
        document(%{
          "opakeVersion" => 999,
          "encryption" => %{
            "$type" => "at.opake.document#directEncryption",
            "envelope" => %{
              "algo" => "future-cipher-v9",
              "nonce" => bytes(),
              "keys" => [wrapped_key("future-wrap-v9")]
            }
          }
        })

      process(rkey, future)

      assert %{record_jsonb: %{"opakeVersion" => 999}} = RecordQueries.lookup(uri(rkey))
      assert_receive {:sse_event, "at.opake.document:upsert", %{record: %{"opakeVersion" => 999}}}
    end
  end
end
