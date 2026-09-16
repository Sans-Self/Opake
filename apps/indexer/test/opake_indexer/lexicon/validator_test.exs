defmodule OpakeIndexer.Lexicon.ValidatorTest do
  @moduledoc """
  Unit coverage for the ingest gate (`record-validity` § indexer validates
  structure for all versions and vocabulary for known versions).

  Pure — no DB, no firehose. Drives `Validator.validate/2` directly with
  hand-built record maps in the shape Jetstream delivers (camelCase, `$bytes` /
  `$link` / `$type` conventions).
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.Lexicon.Validator

  # -- Fixtures (well-formed v1 records) ------------------------------

  defp bytes, do: %{"$bytes" => "AA"}

  defp wrapped_key(algo \\ "x25519-mlkem768-hkdf-a256kw-v2") do
    %{"did" => "did:plc:x", "ciphertext" => bytes(), "algo" => algo}
  end

  defp encrypted_metadata, do: %{"ciphertext" => bytes(), "nonce" => bytes()}

  defp grant(overrides \\ %{}) do
    Map.merge(
      %{
        "opakeVersion" => 1,
        "document" => "at://did:plc:x/at.opake.document/d",
        "recipient" => "did:plc:bob",
        "wrappedKey" => wrapped_key(),
        "encryptedMetadata" => encrypted_metadata(),
        "createdAt" => "2026-01-01T00:00:00Z"
      },
      overrides
    )
  end

  defp keyring(overrides \\ %{}) do
    Map.merge(
      %{
        "opakeVersion" => 1,
        "algo" => "aes-256-gcm",
        "members" => [%{"did" => "did:plc:x", "wrappedKey" => wrapped_key(), "role" => "manager"}],
        "encryptedMetadata" => encrypted_metadata(),
        "createdAt" => "2026-01-01T00:00:00Z"
      },
      overrides
    )
  end

  defp directory(overrides \\ %{}) do
    Map.merge(
      %{
        "opakeVersion" => 1,
        "keyWrapping" => %{
          "$type" => "at.opake.defs#directKeyWrapping",
          "keys" => [wrapped_key()]
        },
        "encryptedMetadata" => encrypted_metadata(),
        "entries" => [],
        "createdAt" => "2026-01-01T00:00:00Z"
      },
      overrides
    )
  end

  defp document(overrides \\ %{}) do
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
          "envelope" => %{
            "algo" => "aes-256-gcm",
            "nonce" => bytes(),
            "keys" => [wrapped_key()]
          }
        },
        "encryptedMetadata" => encrypted_metadata(),
        "createdAt" => "2026-01-01T00:00:00Z"
      },
      overrides
    )
  end

  describe "well-formed known-version records pass" do
    test "grant" do
      assert Validator.validate("at.opake.grant", grant()) == :ok
    end

    test "keyring" do
      assert Validator.validate("at.opake.keyring", keyring()) == :ok
    end

    test "directory (direct key wrapping)" do
      assert Validator.validate("at.opake.directory", directory()) == :ok
    end

    test "directory (keyring key wrapping — no algo vocabulary to check)" do
      dir =
        directory(%{
          "keyWrapping" => %{
            "$type" => "at.opake.defs#keyringKeyWrapping",
            "keyringRef" => %{
              "keyring" => "at://did:plc:x/at.opake.keyring/genesis",
              "wrappedContentKey" => bytes(),
              "rotation" => 0
            }
          }
        })

      assert Validator.validate("at.opake.directory", dir) == :ok
    end

    test "document" do
      assert Validator.validate("at.opake.document", document()) == :ok
    end

    test "unknown extra fields are tolerated (additive evolution)" do
      assert Validator.validate("at.opake.grant", grant(%{"futureField" => "whatever"})) == :ok
    end
  end

  describe "structural failures are refused for all versions" do
    test "missing a required crypto-envelope field (directory keyWrapping)" do
      dir = Map.delete(directory(), "keyWrapping")

      assert {:refused, {:malformed, {:missing_required, "keyWrapping"}}} =
               Validator.validate("at.opake.directory", dir)
    end

    test "missing opakeVersion is malformed, never defaulted" do
      assert {:refused, {:malformed, {:missing_required, "opakeVersion"}}} =
               Validator.validate("at.opake.grant", Map.delete(grant(), "opakeVersion"))
    end

    test "non-integer opakeVersion is malformed" do
      assert {:refused, {:malformed, {"opakeVersion", {:not_an_integer, "1"}}}} =
               Validator.validate("at.opake.grant", grant(%{"opakeVersion" => "1"}))
    end

    test "wrong-typed known field is malformed (keyring members not an array)" do
      assert {:refused, {:malformed, {"members", _}}} =
               Validator.validate("at.opake.keyring", keyring(%{"members" => "nope"}))
    end

    test "unknown union variant is malformed" do
      dir = directory(%{"keyWrapping" => %{"$type" => "at.opake.defs#quantumWrapping"}})

      assert {:refused, {:malformed, {"keyWrapping", {:unknown_union_variant, _}}}} =
               Validator.validate("at.opake.directory", dir)
    end

    test "missing required field inside a nested ref is malformed (wrappedKey has no algo)" do
      bad_key = %{"did" => "did:plc:x", "ciphertext" => bytes()}

      assert {:refused, {:malformed, _}} =
               Validator.validate("at.opake.grant", grant(%{"wrappedKey" => bad_key}))
    end
  end

  describe "future versions: structural only, relayed verbatim" do
    test "well-formed future-version record passes (vocabulary not enforced)" do
      # opakeVersion above the known range, using an algo the indexer has never
      # heard of. Structural floor holds → indexed and relayed verbatim.
      doc =
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

      assert Validator.validate("at.opake.document", doc) == :ok
    end

    test "claimed future version is no structural bypass" do
      # opakeVersion 999 but missing the crypto envelope floor → still malformed.
      dir = directory(%{"opakeVersion" => 999}) |> Map.delete("keyWrapping")

      assert {:refused, {:malformed, {:missing_required, "keyWrapping"}}} =
               Validator.validate("at.opake.directory", dir)
    end
  end

  describe "vocabulary violations are refused for known versions" do
    test "unknown keyring member role" do
      bad =
        keyring(%{
          "members" => [
            %{"did" => "did:plc:x", "wrappedKey" => wrapped_key(), "role" => "superuser"}
          ]
        })

      assert {:refused, {:vocabulary, {"keyringMemberRole", "superuser"}}} =
               Validator.validate("at.opake.keyring", bad)
    end

    test "unknown key-wrap algorithm on a grant" do
      assert {:refused, {:vocabulary, {"keyWrapAlgo", "rot13"}}} =
               Validator.validate(
                 "at.opake.grant",
                 grant(%{"wrappedKey" => wrapped_key("rot13")})
               )
    end

    test "unknown content-encryption algorithm on a document" do
      doc =
        document(%{
          "encryption" => %{
            "$type" => "at.opake.document#directEncryption",
            "envelope" => %{"algo" => "des", "nonce" => bytes(), "keys" => [wrapped_key()]}
          }
        })

      assert {:refused, {:vocabulary, {"contentEncryptionAlgo", "des"}}} =
               Validator.validate("at.opake.document", doc)
    end

    test "unknown key-wrap algo nested in a directory's direct wrapping" do
      dir =
        directory(%{
          "keyWrapping" => %{
            "$type" => "at.opake.defs#directKeyWrapping",
            "keys" => [wrapped_key("rot13")]
          }
        })

      assert {:refused, {:vocabulary, {"keyWrapAlgo", "rot13"}}} =
               Validator.validate("at.opake.directory", dir)
    end
  end

  describe "explicit member validation" do
    test "rejects the prior pre-v1 wrap-only member shape" do
      prior_draft = %{"wrappedKey" => wrapped_key(), "role" => "manager"}

      assert {:refused, {:malformed, {"members", _}}} =
               Validator.validate("at.opake.keyring", keyring(%{"members" => [prior_draft]}))
    end

    test "accepts an admitted member without a current wrap" do
      assert :ok =
               Validator.validate(
                 "at.opake.keyring",
                 keyring(%{"members" => [%{"did" => "did:plc:offline", "role" => "viewer"}]})
               )
    end

    test "rejects duplicate DIDs, a wrap for another DID, and malformed approval bytes" do
      member = %{"did" => "did:plc:alice", "role" => "viewer"}

      assert {:refused, {:malformed, :duplicate_member_did}} =
               Validator.validate("at.opake.keyring", keyring(%{"members" => [member, member]}))

      mismatch = %{
        "did" => "did:plc:alice",
        "role" => "viewer",
        "wrappedKey" => wrapped_key()
      }

      assert {:refused, {:malformed, :member_wrap_did_mismatch}} =
               Validator.validate("at.opake.keyring", keyring(%{"members" => [mismatch]}))

      malformed_approval =
        Map.put(member, "unverifiedKeyApproval", %{"$bytes" => Base.encode64(<<0::248>>)})

      assert {:refused,
              {:malformed,
               {"members", {"unverifiedKeyApproval", {:below_minimum_byte_length, 31}}}}} =
               Validator.validate(
                 "at.opake.keyring",
                 keyring(%{"members" => [malformed_approval]})
               )
    end
  end
end
