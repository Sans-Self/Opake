defmodule OpakeIndexer.Auth.PublicKeyTranscriptTest do
  use ExUnit.Case, async: true

  alias OpakeIndexer.Auth.PublicKeyTranscript

  test "matches the Rust wire-frozen v1 signature transcript vector" do
    transcript =
      PublicKeyTranscript.signature(0x0102_0304, "did:plc:alice", [
        <<0x00, 0xFF>>,
        "x25519",
        <<0x10, 0x11, 0x12>>,
        "ml-kem-768",
        <<0xAA, 0xBB>>,
        "ed25519",
        "2026-09-12T00:00:00Z"
      ])

    assert Base.encode16(transcript, case: :lower) ==
             "61742e6f70616b652e7075626c69634b65792f73656c663a763136393039303630090000000d0000006469643a706c633a616c69636504000000040302010200000000ff06000000783235353139030000001011120a0000006d6c2d6b656d2d37363802000000aabb070000006564323535313914000000323032362d30392d31325430303a30303a30305a"
  end
end
