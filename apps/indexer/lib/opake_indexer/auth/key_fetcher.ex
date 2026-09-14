defmodule OpakeIndexer.Auth.KeyFetcher do
  @moduledoc """
  Resolves a DID to its Ed25519 signing public key by walking the chain:
  DID → DID document → PDS service endpoint → `at.opake.publicKey/self` record
  → `signingKey.$bytes` (32-byte Ed25519 key).

  Supports `did:plc:` (via the configured PLC directory) and `did:web:`
  (via .well-known). Implements `KeyFetcherBehaviour` so tests can
  substitute a Mox mock.
  """

  @behaviour OpakeIndexer.Auth.KeyFetcherBehaviour

  require Logger

  alias OpakeIndexer.Lexicon.Vocabulary

  # Compressed encodings of curve25519-dalek's `constants::EIGHT_TORSION`
  # (the eight canonical small-order points), followed by the six
  # non-canonical spellings of those same points that a decoder reducing
  # modulo p accepts: the sign-bit variants of 1 and 0, and p, p+1 with and
  # without the sign bit. OTP's `:crypto.verify` accepts the identity
  # public-key/R forgery, whereas Rust's `VerifyingKey::verify_strict`
  # rejects any small-order point.
  @small_order_ed25519_points for hex <- ~w(
                                0100000000000000000000000000000000000000000000000000000000000000
                                c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a
                                0000000000000000000000000000000000000000000000000000000000000080
                                26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05
                                ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f
                                26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85
                                0000000000000000000000000000000000000000000000000000000000000000
                                c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa
                                0100000000000000000000000000000000000000000000000000000000000080
                                ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                                edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f
                                edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                                eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f
                                eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                              ),
                                  do: Base.decode16!(hex, case: :lower)

  @impl true
  @spec fetch_authentication_decision(String.t()) ::
          {:ok, OpakeIndexer.Auth.KeyFetcherBehaviour.decision()} | {:error, term()}
  def fetch_authentication_decision(did) do
    with {:ok, did_doc} <- resolve_did_document(did),
         {:ok, pds_url} <- extract_pds_url(did_doc),
         {:ok, record} <- fetch_public_key_record(pds_url, did),
         {:ok, pubkey_bytes, verified} <- validate_record_with_verification(did, did_doc, record),
         {:ok, anchor_history} <- read_anchor_history(did, did_doc, pubkey_bytes) do
      {:ok, %{key: pubkey_bytes, verified: verified, anchor_history: anchor_history}}
    end
  end

  defp resolve_did_document("did:plc:" <> _ = did) do
    url = "#{plc_directory_url()}/#{did}"
    fetch_json(url, :did_document)
  end

  defp resolve_did_document("did:web:" <> host) do
    url = "https://#{host}/.well-known/did.json"
    fetch_json(url, :did_document)
  end

  defp resolve_did_document(_did), do: {:error, "unsupported DID method"}

  defp extract_pds_url(did_doc) do
    services = did_doc["service"] || []

    pds_service =
      Enum.find(services, fn svc ->
        svc["id"] == "#atproto_pds" or svc["type"] == "AtprotoPersonalDataServer"
      end)

    case pds_service do
      %{"serviceEndpoint" => endpoint} when is_binary(endpoint) -> {:ok, endpoint}
      _ -> {:error, "no PDS service in DID document"}
    end
  end

  defp fetch_public_key_record(pds_url, did) do
    url =
      "#{pds_url}/xrpc/com.atproto.repo.getRecord?" <>
        URI.encode_query(repo: did, collection: "at.opake.publicKey", rkey: "self")

    case fetch_json(url, :public_key_record) do
      {:ok, %{"value" => record}} when is_map(record) -> {:ok, record}
      {:ok, _} -> {:error, {:invalid, :public_key_record_missing}}
      error -> error
    end
  end

  @doc false
  # Pure boundary for signed-record regression vectors. An absent anchor keeps
  # the explicitly unverified first-publication contract: a future record only
  # needs a valid signing key. An account that declares #opake must present the
  # v1 account-bound signature the indexer knows how to verify.
  def validate_record(did, did_doc, record) when is_map(record) do
    with {:ok, signing, _verified} <- validate_record_with_verification(did, did_doc, record) do
      {:ok, signing}
    end
  end

  defp validate_record_with_verification(did, did_doc, record) when is_map(record) do
    with {:ok, signing} <- decode_bytes(get_in(record, ["signingKey", "$bytes"]), 32),
         {:ok, anchor} <- opake_anchor(did_doc, did) do
      cond do
        not is_nil(anchor) ->
          with :ok <- validate_encryption_bundle(record),
               {:ok, ^signing} <- verify_anchored_record(record, did, signing, anchor) do
            {:ok, signing, true}
          end

        # An unverified record of a version this build understands still has
        # to present a well-formed encryption bundle; only a record from a
        # future version is authenticated on its signing key alone.
        understood_version?(record) ->
          with :ok <- validate_encryption_bundle(record), do: {:ok, signing, false}

        true ->
          {:ok, signing, false}
      end
    end
  end

  defp validate_record_with_verification(_, _, _), do: {:error, {:invalid, :public_key_record}}

  defp understood_version?(%{"opakeVersion" => version}) when is_integer(version),
    do: version <= 1

  defp understood_version?(_), do: true

  defp validate_encryption_bundle(%{"opakeVersion" => version})
       when is_integer(version) and version > 1,
       do: {:error, {:invalid, :unsupported_anchored_public_key_version}}

  defp validate_encryption_bundle(%{"opakeVersion" => version} = record) when version == 1 do
    with true <- Vocabulary.permits?("publicKeyAlgo", version, record["x25519Algo"]),
         true <- Vocabulary.permits?("publicKeyAlgo", version, record["mlKemAlgo"]),
         true <- optional_vocabulary?("publicKeyAlgo", version, record["signingAlgo"]),
         true <- optional_vocabulary?("publicKeySignatureAlgo", version, record["signatureAlgo"]),
         {:ok, _} <- decode_bytes(get_in(record, ["x25519PublicKey", "$bytes"]), 32),
         {:ok, _} <- decode_bytes(get_in(record, ["mlKemPublicKey", "$bytes"]), 1184),
         true <- is_binary(record["createdAt"]) do
      :ok
    else
      _ -> {:error, {:invalid, :public_key_record}}
    end
  end

  defp validate_encryption_bundle(_), do: {:error, {:invalid, :public_key_record_version}}

  defp optional_vocabulary?(_field, _version, nil), do: true

  defp optional_vocabulary?(field, version, value) when is_binary(value),
    do: Vocabulary.permits?(field, version, value)

  defp optional_vocabulary?(_field, _version, _value), do: false

  defp verify_anchored_record(record, did, signing, anchor) do
    with "ed25519" <- record["signingAlgo"],
         "ed25519" <- record["signatureAlgo"],
         true <- signing == anchor,
         {:ok, signature} <- decode_bytes(get_in(record, ["signature", "$bytes"]), 64),
         {:ok, x25519} <- decode_bytes(get_in(record, ["x25519PublicKey", "$bytes"]), 32),
         {:ok, ml_kem} <- decode_bytes(get_in(record, ["mlKemPublicKey", "$bytes"]), 1184) do
      transcript =
        OpakeIndexer.Auth.PublicKeyTranscript.signature(record["opakeVersion"], did, [
          x25519,
          record["x25519Algo"],
          ml_kem,
          record["mlKemAlgo"],
          signing,
          record["signingAlgo"],
          record["createdAt"]
        ])

      if strict_ed25519_verify(anchor, transcript, signature),
        do: {:ok, signing},
        else: {:error, {:invalid, :account_public_key_signature}}
    else
      _ -> {:error, {:invalid, :account_public_key_signature}}
    end
  end

  @doc false
  # Keep this public pure validation boundary so it can be tested without a
  # network DID resolution.  A named #opake method is a security assertion:
  # malformed, foreign, unsupported, and duplicate declarations all fail
  # closed; only complete absence permits an unverified record.
  def opake_anchor(did_doc, did) when is_map(did_doc) and is_binary(did) do
    with ^did <- did_doc["id"],
         methods when is_list(methods) <- Map.get(did_doc, "verificationMethod", []) do
      matches =
        Enum.filter(methods, &(is_map(&1) and &1["id"] in ["#opake", did <> "#opake"]))

      case matches do
        [] -> {:ok, nil}
        [method] -> validate_opake_method(method, did)
        _ -> {:error, "duplicate #opake verification method"}
      end
    else
      _ -> {:error, "malformed DID document"}
    end
  end

  def opake_anchor(_, _), do: {:error, "malformed DID document"}

  defp validate_opake_method(
         %{
           "controller" => did,
           "type" => type,
           "publicKeyMultibase" => "z" <> encoded
         },
         did
       )
       when type in ["Multikey", "Ed25519VerificationKey2020"] do
    decode_ed25519_multibase(encoded)
  end

  defp validate_opake_method(_, _), do: {:error, "malformed #opake verification method"}

  defp decode_ed25519_multibase(encoded) do
    case base58_decode(encoded) do
      {:ok, <<0xED, 0x01, key::binary-size(32)>>} -> {:ok, key}
      _ -> {:error, "unsupported or malformed #opake key"}
    end
  end

  defp base58_decode(encoded) do
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

    if encoded == "" or byte_size(encoded) > 100 do
      {:error, "invalid multibase key"}
    else
      leading_zeros = encoded |> :binary.bin_to_list() |> Enum.take_while(&(&1 == ?1)) |> length()

      Enum.reduce_while(:binary.bin_to_list(encoded), {:ok, 0}, fn char, {:ok, value} ->
        case :binary.match(alphabet, <<char>>) do
          {index, 1} -> {:cont, {:ok, value * 58 + index}}
          :nomatch -> {:halt, {:error, "invalid multibase key"}}
        end
      end)
      |> case do
        {:ok, value} ->
          significant = if value == 0, do: <<>>, else: :binary.encode_unsigned(value)
          {:ok, :binary.copy(<<0>>, leading_zeros) <> significant}

        error ->
          error
      end
    end
  end

  defp decode_bytes(bytes_b64, expected_size) when is_binary(bytes_b64) do
    with {:ok, bytes} <- OpakeIndexer.Auth.Base64.decode(bytes_b64) do
      if byte_size(bytes) == expected_size do
        {:ok, bytes}
      else
        {:error, "key must be #{expected_size} bytes, got #{byte_size(bytes)}"}
      end
    end
  end

  defp decode_bytes(_, _), do: {:error, "missing byte field"}

  @doc false
  # Public so the shared cross-language vectors exercise the production
  # verifier rather than a re-implementation of it.
  @spec strict_ed25519_verify(binary(), binary(), binary()) :: boolean()
  def strict_ed25519_verify(pubkey, message, signature)
      when byte_size(pubkey) == 32 and byte_size(signature) == 64 do
    <<r::binary-size(32), _s::binary-size(32)>> = signature

    not small_order_point?(pubkey) and not small_order_point?(r) and
      :crypto.verify(:eddsa, :none, message, signature, [pubkey, :ed25519])
  rescue
    _ -> false
  end

  def strict_ed25519_verify(_, _, _), do: false

  defp small_order_point?(point), do: point in @small_order_ed25519_points

  @doc false
  # Exposed so regression vectors run against the production list rather than
  # a hand-copied excerpt of it.
  @spec small_order_points() :: [binary()]
  def small_order_points, do: @small_order_ed25519_points

  defp plc_directory_url do
    Application.fetch_env!(:opake_indexer, :plc_directory_url)
  end

  # The history result is a notice, never a source of current authority. We
  # still read it with the same cache lifetime as the signing-key decision so
  # all account resolvers consume the required audit history. A transport
  # failure on the audit log leaves a verified account verified: the history is
  # reported as unreadable rather than as an absence of replacement.
  defp read_anchor_history("did:plc:" <> _ = did, did_doc, signing) do
    case opake_anchor(did_doc, did) do
      {:ok, nil} ->
        {:ok, :no_history}

      {:ok, _anchor} ->
        url = "#{plc_directory_url()}/#{did}/log/audit"

        case fetch_json(url, :plc_audit_history) do
          {:ok, operations} when is_list(operations) ->
            history_notice(did, operations, signing)

          {:ok, _} ->
            {:error, {:invalid, :plc_audit_history}}

          {:error, {:unavailable, _source}} ->
            {:ok, :unavailable}

          error ->
            error
        end

      error ->
        error
    end
  end

  defp read_anchor_history("did:web:" <> _, _did_doc, _signing), do: {:ok, :no_history}

  defp read_anchor_history(_, _did_doc, _signing),
    do: {:error, {:invalid, :unsupported_did_method}}

  @doc false
  @spec history_notice(String.t(), [map()], binary()) ::
          {:ok, OpakeIndexer.Auth.KeyFetcherBehaviour.anchor_history()} | {:error, term()}
  def history_notice("did:web:" <> _, _operations, _current), do: {:ok, :no_history}

  def history_notice("did:plc:" <> _, operations, current),
    do: audit_anchor_history(operations, current)

  def history_notice(_, _operations, _current), do: {:error, {:invalid, :unsupported_did_method}}

  @doc false
  @spec audit_anchor_history([map()], binary()) ::
          {:ok, :replaced | :not_replaced} | {:error, term()}
  def audit_anchor_history(operations, current)
      when is_list(operations) and byte_size(current) == 32 do
    Enum.reduce_while(operations, {:ok, false}, fn envelope, {:ok, replaced} ->
      case envelope do
        %{"operation" => operation} when is_map(operation) ->
          case operation do
            %{"verificationMethods" => methods} when is_map(methods) ->
              case Map.fetch(methods, "opake") do
                :error ->
                  {:cont, {:ok, replaced}}

                {:ok, "did:key:z" <> encoded} ->
                  case decode_ed25519_multibase(encoded) do
                    {:ok, key} -> {:cont, {:ok, replaced or key != current}}
                    _ -> {:halt, {:error, {:invalid, :plc_audit_history}}}
                  end

                {:ok, _} ->
                  {:halt, {:error, {:invalid, :plc_audit_history}}}
              end

            %{"verificationMethods" => _} ->
              {:halt, {:error, {:invalid, :plc_audit_history}}}

            %{} ->
              {:cont, {:ok, replaced}}
          end

        _ ->
          {:halt, {:error, {:invalid, :plc_audit_history}}}
      end
    end)
    |> case do
      {:ok, true} -> {:ok, :replaced}
      {:ok, false} -> {:ok, :not_replaced}
      error -> error
    end
  end

  def audit_anchor_history(_, _), do: {:error, {:invalid, :plc_audit_history}}

  defp fetch_json(url, source) do
    case Req.get(url) do
      {:ok, %Req.Response{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %Req.Response{status: 200, body: body}}
      when is_list(body) and source == :plc_audit_history ->
        {:ok, body}

      # PLC directory returns application/did+ld+json which Req doesn't auto-decode
      {:ok, %Req.Response{status: 200, body: body}} when is_binary(body) ->
        case Jason.decode(body) do
          {:ok, decoded} when is_map(decoded) -> {:ok, decoded}
          {:ok, decoded} when is_list(decoded) and source == :plc_audit_history -> {:ok, decoded}
          _ -> {:error, {:invalid, source}}
        end

      {:ok, %Req.Response{status: status}} when status >= 500 ->
        {:error, {:unavailable, source}}

      {:ok, %Req.Response{status: 429}} ->
        {:error, {:unavailable, source}}

      {:ok, %Req.Response{}} ->
        {:error, {:invalid, source}}

      {:error, _reason} ->
        {:error, {:unavailable, source}}
    end
  end
end
