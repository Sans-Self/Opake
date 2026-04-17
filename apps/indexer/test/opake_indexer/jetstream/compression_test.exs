defmodule OpakeIndexer.Jetstream.CompressionTest do
  @moduledoc """
  Round-trip + dict-ID-validation tests for the zstd decompression module.

  We can't test against live Jetstream in CI, so instead we compress a
  known JSON frame *with the same dictionary* the consumer uses for
  decoding and verify the round trip. If the dictionary is loadable
  and ezstd is wired up correctly, this passes; if either is broken,
  the production decoder is also broken.
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.Jetstream.Compression

  setup_all do
    # init/0 is idempotent — safe to call from multiple test files.
    Compression.init()
    :ok
  end

  defp sample_frame_json do
    Jason.encode!(%{
      "kind" => "commit",
      "did" => "did:plc:test1234567890",
      "time_us" => 1_709_330_400_000_000,
      "commit" => %{
        "rev" => "3l3qo2vutsw2b",
        "operation" => "create",
        "collection" => "app.bsky.feed.post",
        "rkey" => "3kabcdef",
        "cid" => "bafyreih",
        "record" => %{
          "$type" => "app.bsky.feed.post",
          "createdAt" => "2026-04-07T12:00:00.000Z",
          "text" => "hello firehose",
          "langs" => ["en"]
        }
      }
    })
  end

  defp dict_bytes do
    File.read!(Application.app_dir(:opake_indexer, "priv/jetstream_zstd_dict"))
  end

  defp compress_with_dict(payload) do
    cdict = :ezstd.create_cdict(dict_bytes(), 3)
    :ezstd.compress_using_cdict(payload, cdict)
  end

  test "decompress/2 round-trips a frame compressed with the same dictionary" do
    json = sample_frame_json()
    compressed = compress_with_dict(json)

    # Sanity: the compressed payload is smaller than the original.
    # (For a single small frame the dictionary is what makes this work
    # at all — without the dict, zstd can't find enough repetition.)
    assert byte_size(compressed) < byte_size(json)

    ctx = Compression.new_context()
    assert {:ok, decompressed} = Compression.decompress(ctx, compressed)
    assert decompressed == json
  end

  test "decompress/2 reuses one context across multiple frames" do
    # Critical: the streaming context must reset cleanly between frames
    # so that state from one decompression doesn't bleed into the next.
    ctx = Compression.new_context()
    json1 = sample_frame_json()
    json2 = Jason.encode!(%{"kind" => "commit", "did" => "did:plc:other"})

    f1 = compress_with_dict(json1)
    f2 = compress_with_dict(json2)

    assert {:ok, ^json1} = Compression.decompress(ctx, f1)
    assert {:ok, ^json2} = Compression.decompress(ctx, f2)
    assert {:ok, ^json1} = Compression.decompress(ctx, f1)
  end

  test "validate_frame!/1 succeeds when frame matches the expected dict id" do
    compressed = compress_with_dict(sample_frame_json())
    assert :ok = Compression.validate_frame!(compressed)
  end

  test "validate_frame!/1 raises a clear error on dict id mismatch" do
    # We can't use ezstd.train_dictionary (not exposed) and random bytes
    # don't yield a parseable dict. Instead we surgically patch the
    # dictionary_id field of a real compressed frame.
    #
    # zstd frame layout (from RFC 8478):
    #   bytes 0..3:   magic 0x28B52FFD
    #   byte 4:       Frame_Header_Descriptor
    #     - bits 0..1: Dictionary_ID_flag (3 = 4-byte ID)
    #     - bit 5:     Single_Segment_flag (1 = no window descriptor)
    #   byte 5..N:    optional window descriptor
    #   then:         Dictionary_ID (1/2/4 bytes per the flag)
    #
    # Our bsky dict ID is > 65535 so it's encoded as 4 little-endian
    # bytes. Find it and overwrite it with a different value.
    real_frame = compress_with_dict(sample_frame_json())
    expected_id_bytes = <<Compression.expected_dict_id()::little-32>>

    {prefix_len, _} =
      :binary.match(real_frame, expected_id_bytes) ||
        flunk("could not locate dict id in compressed frame — frame layout assumption broken")

    bogus_id = 999_999
    bogus_id_bytes = <<bogus_id::little-32>>

    bogus_frame =
      binary_part(real_frame, 0, prefix_len) <>
        bogus_id_bytes <>
        binary_part(
          real_frame,
          prefix_len + 4,
          byte_size(real_frame) - prefix_len - 4
        )

    # Sanity: ezstd reads the patched ID back as our bogus value.
    assert :ezstd.get_dict_id_from_frame(bogus_frame) == bogus_id

    assert_raise RuntimeError, ~r/unexpected zstd dictionary/, fn ->
      Compression.validate_frame!(bogus_frame)
    end
  end

  test "expected_dict_id/0 matches the vendored dictionary's embedded id" do
    ddict = :ezstd.create_ddict(dict_bytes())
    file_id = :ezstd.get_dict_id_from_ddict(ddict)

    assert Compression.expected_dict_id() == file_id
  end

  test "init/0 is idempotent" do
    assert :ok = Compression.init()
    assert :ok = Compression.init()
    assert :ok = Compression.init()
  end
end
