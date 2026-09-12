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

  @impl true
  def fetch_signing_key(did) do
    with {:ok, did_doc} <- resolve_did_document(did),
         {:ok, pds_url} <- extract_pds_url(did_doc),
         {:ok, record} <- fetch_public_key_record(pds_url, did),
         {:ok, pubkey_bytes} <- validate_record(did, did_doc, record) do
      {:ok, pubkey_bytes}
    end
  end

  defp resolve_did_document("did:plc:" <> _ = did) do
    url = "#{plc_directory_url()}/#{did}"
    fetch_json(url)
  end

  defp resolve_did_document("did:web:" <> host) do
    url = "https://#{host}/.well-known/did.json"
    fetch_json(url)
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

    with {:ok, %{"value" => record}} <- fetch_json(url), do: {:ok, record}
  end

  defp validate_record(did, did_doc, record) do
    # Classify the declared schema and algorithms before considering whether a
    # DID document has an anchor.  An absent anchor permits an explicitly
    # unverified first publication, not an unknown record format.
    with :ok <- validate_encryption_bundle(record),
         {:ok, signing} <- decode_bytes(get_in(record, ["signingKey", "$bytes"]), 32),
         {:ok, anchor} <- opake_anchor(did_doc, did) do
      if is_nil(anchor),
        do: {:ok, signing},
        else: verify_anchored_record(record, did, signing, anchor)
    end
  end

  defp validate_encryption_bundle(%{"opakeVersion" => version})
       when is_integer(version) and version > 1,
       do: {:error, "unsupported newer public-key record version"}

  defp validate_encryption_bundle(
         %{"opakeVersion" => 1, "x25519Algo" => "x25519", "mlKemAlgo" => "ml-kem-768"} = record
       ) do
    with {:ok, _} <- decode_bytes(get_in(record, ["x25519PublicKey", "$bytes"]), 32),
         {:ok, _} <- decode_bytes(get_in(record, ["mlKemPublicKey", "$bytes"]), 1184),
         true <- record["signingAlgo"] in [nil, "ed25519"],
         true <- record["signatureAlgo"] in [nil, "ed25519"],
         true <- is_binary(record["createdAt"]) do
      :ok
    else
      _ -> {:error, "invalid public-key record"}
    end
  end

  defp validate_encryption_bundle(%{"opakeVersion" => 1}),
    do: {:error, "invalid public-key record vocabulary"}

  defp validate_encryption_bundle(_), do: {:error, "invalid public-key record version"}

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

      if :crypto.verify(:eddsa, :none, transcript, signature, [anchor, :ed25519]),
        do: {:ok, signing},
        else: {:error, "invalid account public-key signature"}
    else
      _ -> {:error, "invalid account public-key signature"}
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

  defp plc_directory_url do
    Application.fetch_env!(:opake_indexer, :plc_directory_url)
  end

  defp fetch_json(url) do
    case Req.get(url) do
      {:ok, %Req.Response{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      # PLC directory returns application/did+ld+json which Req doesn't auto-decode
      {:ok, %Req.Response{status: 200, body: body}} when is_binary(body) ->
        case Jason.decode(body) do
          {:ok, decoded} when is_map(decoded) -> {:ok, decoded}
          _ -> {:error, "failed to decode JSON from #{url}"}
        end

      {:ok, %Req.Response{status: status}} ->
        {:error, "HTTP #{status} from #{url}"}

      {:error, reason} ->
        {:error, "failed to fetch #{url}: #{inspect(reason)}"}
    end
  end
end
