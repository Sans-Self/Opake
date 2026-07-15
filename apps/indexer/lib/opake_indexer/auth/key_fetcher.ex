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
    with {:ok, pds_url} <- resolve_pds(did),
         {:ok, pubkey_bytes} <- fetch_public_key_record(pds_url, did) do
      {:ok, pubkey_bytes}
    end
  end

  defp resolve_pds(did) do
    with {:ok, did_doc} <- resolve_did_document(did) do
      extract_pds_url(did_doc)
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

    with {:ok, response} <- fetch_json(url) do
      extract_signing_key(response)
    end
  end

  defp extract_signing_key(response) do
    case get_in(response, ["value", "signingKey", "$bytes"]) do
      bytes_b64 when is_binary(bytes_b64) ->
        decode_pubkey_bytes(bytes_b64)

      _ ->
        {:error, "signingKey.$bytes not found in publicKey record"}
    end
  end

  defp decode_pubkey_bytes(bytes_b64) do
    with {:ok, bytes} <- OpakeIndexer.Auth.Base64.decode(bytes_b64) do
      if byte_size(bytes) == 32 do
        {:ok, bytes}
      else
        {:error, "signing key must be 32 bytes, got #{byte_size(bytes)}"}
      end
    end
  end

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
