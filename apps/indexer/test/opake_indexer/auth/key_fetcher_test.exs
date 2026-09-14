defmodule OpakeIndexer.Auth.KeyFetcherTest do
  use ExUnit.Case, async: false

  alias OpakeIndexer.Auth.KeyFetcher

  @did "did:plc:alice"

  # The accept/reject boundary is shared with opake-core so both verifiers are
  # held to one answer per adversarial form.
  @vectors_path Path.expand(
                  Path.join(
                    __DIR__,
                    "../../../../../tests/vectors/ed25519-signature-vectors.json"
                  )
                )
  @external_resource @vectors_path
  @vectors Jason.decode!(File.read!(@vectors_path))

  defp vector(name), do: Enum.find(@vectors["vectors"], &(&1["name"] == name))

  defp unhex(hex), do: Base.decode16!(hex, case: :lower)

  defp document(methods \\ []), do: %{"id" => @did, "verificationMethod" => methods}

  defp bytes(bytes), do: %{"$bytes" => Base.encode64(bytes)}

  defp base58(bytes) do
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    zeros = bytes |> :binary.bin_to_list() |> Enum.take_while(&(&1 == 0)) |> length()

    digits =
      case :binary.decode_unsigned(bytes) do
        0 -> ""
        value -> base58_digits(value, alphabet, [])
      end

    String.duplicate("1", zeros) <> digits
  end

  defp base58_digits(0, _alphabet, acc), do: IO.iodata_to_binary(acc)

  defp base58_digits(value, alphabet, acc) do
    base58_digits(div(value, 58), alphabet, [binary_part(alphabet, rem(value, 58), 1) | acc])
  end

  defp anchored_document(pubkey, did \\ @did) do
    method = %{
      "id" => "#opake",
      "controller" => did,
      "type" => "Multikey",
      "publicKeyMultibase" => "z" <> base58(<<0xED, 0x01>> <> pubkey)
    }

    %{"id" => did, "verificationMethod" => [method]}
  end

  defp signed_record do
    {pubkey, private_key} = :crypto.generate_key(:eddsa, :ed25519)
    x25519 = :binary.copy(<<1>>, 32)
    ml_kem = :binary.copy(<<2>>, 1184)

    record = %{
      "opakeVersion" => 1,
      "x25519PublicKey" => bytes(x25519),
      "x25519Algo" => "x25519",
      "mlKemPublicKey" => bytes(ml_kem),
      "mlKemAlgo" => "ml-kem-768",
      "signingKey" => bytes(pubkey),
      "signingAlgo" => "ed25519",
      "signatureAlgo" => "ed25519",
      "createdAt" => "2026-09-12T00:00:00Z"
    }

    transcript =
      OpakeIndexer.Auth.PublicKeyTranscript.signature(1, @did, [
        x25519,
        "x25519",
        ml_kem,
        "ml-kem-768",
        pubkey,
        "ed25519",
        record["createdAt"]
      ])

    signature = :crypto.sign(:eddsa, :none, transcript, [private_key, :ed25519])
    {Map.put(record, "signature", bytes(signature)), anchored_document(pubkey)}
  end

  test "absence of #opake permits an explicitly unverified first publication" do
    assert {:ok, nil} = KeyFetcher.opake_anchor(document(), @did)
  end

  test "rejects duplicate #opake methods before accepting either" do
    methods = [%{"id" => "#opake"}, %{"id" => @did <> "#opake"}]

    assert {:error, "duplicate #opake verification method"} =
             KeyFetcher.opake_anchor(document(methods), @did)
  end

  test "rejects a #opake method controlled by a different DID" do
    method = %{
      "id" => "#opake",
      "controller" => "did:plc:mallory",
      "type" => "Multikey",
      "publicKeyMultibase" => "z1"
    }

    assert {:error, "malformed #opake verification method"} =
             KeyFetcher.opake_anchor(document([method]), @did)
  end

  test "rejects an unsupported #opake method type" do
    method = %{
      "id" => "#opake",
      "controller" => @did,
      "type" => "P256Key",
      "publicKeyMultibase" => "z1"
    }

    assert {:error, "malformed #opake verification method"} =
             KeyFetcher.opake_anchor(document([method]), @did)
  end

  test "rejects a DID document whose subject differs from the requested DID" do
    assert {:error, "malformed DID document"} =
             KeyFetcher.opake_anchor(%{"id" => "did:plc:mallory"}, @did)
  end

  test "rejects malformed verification-method collections" do
    assert {:error, "malformed DID document"} =
             KeyFetcher.opake_anchor(%{"id" => @did, "verificationMethod" => %{}}, @did)
  end

  test "validates a real account-bound Ed25519 signature" do
    {record, did_doc} = signed_record()
    assert {:ok, _} = KeyFetcher.validate_record(@did, did_doc, record)
  end

  test "resolves the shared adversarial vectors exactly as opake-core does" do
    assert Enum.any?(@vectors["vectors"], &(&1["expected"] == "accept")),
           "the fixture must prove agreement on acceptance, not only on rejection"

    for %{"name" => name, "expected" => expected} = vector <- @vectors["vectors"] do
      accepted =
        KeyFetcher.strict_ed25519_verify(
          unhex(vector["publicKeyHex"]),
          unhex(vector["messageHex"]),
          unhex(vector["signatureHex"])
        )

      assert accepted == (expected == "accept"), "#{name} resolved as #{accepted}"
    end
  end

  test "a real record signature still travels the verifier the vectors exercise" do
    {record, did_doc} = signed_record()

    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyFetcher.validate_record(
               @did,
               did_doc,
               put_in(record, ["signature", "$bytes"], Base.encode64(<<0::512>>))
             )
  end

  test "rejects a vocabulary-valid algorithm substitution with unchanged key bytes" do
    {record, did_doc} = signed_record()

    # Both values are permitted `publicKeyAlgo` vocabulary entries for v1, so
    # this only fails if the signed transcript binds the algorithm label to the
    # otherwise unchanged 32-byte public key.
    substituted = Map.put(record, "x25519Algo", "ed25519")

    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyFetcher.validate_record(@did, did_doc, substituted)
  end

  test "rejects the identity-point forgery that OTP accepts" do
    # OTP's `:crypto.verify` accepts the unadorned identity point for any
    # transcript; the sign-bit spelling it rejects on its own. Both must fail
    # here, so the guard cannot be resting on OTP's own decoder.
    for {vector_name, otp_accepts?} <- [
          {"identity_point_key", true},
          {"identity_point_key_with_sign_bit", false}
        ] do
      identity = unhex(vector(vector_name)["publicKeyHex"])
      {record, _did_doc} = signed_record()

      forged =
        record
        |> put_in(["signingKey", "$bytes"], Base.encode64(identity))
        |> put_in(["signature", "$bytes"], Base.encode64(identity <> <<0::256>>))

      did_doc = anchored_document(identity)

      transcript =
        OpakeIndexer.Auth.PublicKeyTranscript.signature(1, @did, [
          Base.decode64!(forged["x25519PublicKey"]["$bytes"]),
          forged["x25519Algo"],
          Base.decode64!(forged["mlKemPublicKey"]["$bytes"]),
          forged["mlKemAlgo"],
          identity,
          forged["signingAlgo"],
          forged["createdAt"]
        ])

      assert :crypto.verify(
               :eddsa,
               :none,
               transcript,
               Base.decode64!(forged["signature"]["$bytes"]),
               [identity, :ed25519]
             ) == otp_accepts?

      assert {:error, {:invalid, :account_public_key_signature}} =
               KeyFetcher.validate_record(@did, did_doc, forged)
    end
  end

  test "rejects every small-order public-key encoding the production list carries" do
    points = KeyFetcher.small_order_points()
    assert points == Enum.map(@vectors["smallOrderPointsHex"], &unhex/1)
    assert length(points) == 14

    for point <- points do
      {record, _did_doc} = signed_record()

      forged =
        record
        |> put_in(["signingKey", "$bytes"], Base.encode64(point))
        |> put_in(["signature", "$bytes"], Base.encode64(<<1, 0::248, 0::256>>))

      assert {:error, {:invalid, :account_public_key_signature}} =
               KeyFetcher.validate_record(@did, anchored_document(point), forged)
    end
  end

  test "an unverified record of an understood version is still bundle-validated" do
    {record, _did_doc} = signed_record()
    assert {:ok, _} = KeyFetcher.validate_record(@did, document(), record)

    # No anchor, so no signature to check — but a v1 record still has to carry
    # the declared algorithms and correctly sized keys.
    for broken <- [
          put_in(record, ["mlKemPublicKey", "$bytes"], Base.encode64(:binary.copy(<<2>>, 64))),
          Map.put(record, "x25519Algo", "rot13"),
          Map.delete(record, "createdAt")
        ] do
      assert {:error, {:invalid, :public_key_record}} =
               KeyFetcher.validate_record(@did, document(), broken)
    end
  end

  test "future unverified records retain signing-key authentication" do
    {record, _did_doc} = signed_record()

    # A version this build does not understand carries a bundle it cannot
    # judge: authentication falls back to the signing key alone.
    future =
      record
      |> Map.put("opakeVersion", 2)
      |> Map.put("x25519Algo", "future-x25519")
      |> put_in(["mlKemPublicKey", "$bytes"], Base.encode64(:binary.copy(<<2>>, 64)))

    assert {:ok, _} = KeyFetcher.validate_record(@did, document(), future)
  end

  # B7: the signed transcript binds the account DID, and the anchor names the
  # one key that may sign the record.
  test "rejects a record whose signature is bound to a different DID" do
    {record, _did_doc} = signed_record()
    impostor = "did:plc:mallory"
    signing = Base.decode64!(record["signingKey"]["$bytes"])

    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyFetcher.validate_record(
               impostor,
               anchored_document(signing, impostor),
               record
             )
  end

  test "rejects a validly signed record whose signing key is not the anchor" do
    {record, _did_doc} = signed_record()
    {other_key, _private_key} = :crypto.generate_key(:eddsa, :ed25519)

    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyFetcher.validate_record(@did, anchored_document(other_key), record)
  end

  test "fetches a signed anchored record and its JSON audit array as one decision" do
    {record, did_doc} = signed_record()
    key = Base.decode64!(record["signingKey"]["$bytes"])
    anchor = did_doc["verificationMethod"] |> hd() |> Map.fetch!("publicKeyMultibase")

    audit = [
      %{
        "nullified" => true,
        "operation" => %{
          "verificationMethods" => %{
            "opake" => "did:key:" <> anchor
          }
        }
      }
    ]

    {base_url, _server} =
      http_server(3, fn base_url, request ->
        cond do
          String.contains?(request, "/log/audit") ->
            audit

          String.contains?(request, "com.atproto.repo.getRecord") ->
            %{"value" => record}

          true ->
            Map.put(did_doc, "service", [
              %{"type" => "AtprotoPersonalDataServer", "serviceEndpoint" => base_url}
            ])
        end
      end)

    old_directory = Application.get_env(:opake_indexer, :plc_directory_url)
    Application.put_env(:opake_indexer, :plc_directory_url, base_url)

    on_exit(fn -> Application.put_env(:opake_indexer, :plc_directory_url, old_directory) end)

    assert {:ok, %{key: ^key, verified: true, anchor_history: :not_replaced}} =
             KeyFetcher.fetch_authentication_decision(@did)
  end

  # spec:account-verification § Resolution reads the anchor's history and reports a replacement
  test "an unreachable audit log leaves a verified account verified" do
    {record, did_doc} = signed_record()
    key = Base.decode64!(record["signingKey"]["$bytes"])

    # Req retries 5xx by default; the decision under test is the one it reaches
    # after giving up, so skip the backoff rather than sleep through it.
    previous_req_options = Application.get_env(:req, :default_options, [])
    Req.default_options(retry: false)
    on_exit(fn -> Req.default_options(previous_req_options) end)

    {base_url, _server} =
      http_server(3, fn base_url, request ->
        cond do
          String.contains?(request, "/log/audit") ->
            {503, %{"error" => "upstream"}}

          String.contains?(request, "com.atproto.repo.getRecord") ->
            %{"value" => record}

          true ->
            Map.put(did_doc, "service", [
              %{"type" => "AtprotoPersonalDataServer", "serviceEndpoint" => base_url}
            ])
        end
      end)

    old_directory = Application.get_env(:opake_indexer, :plc_directory_url)
    Application.put_env(:opake_indexer, :plc_directory_url, base_url)

    on_exit(fn -> Application.put_env(:opake_indexer, :plc_directory_url, old_directory) end)

    assert {:ok, %{key: ^key, verified: true, anchor_history: :unavailable}} =
             KeyFetcher.fetch_authentication_decision(@did)
  end

  test "audit history reports a nullified replacement but not same-key readdition" do
    {current, _} = :crypto.generate_key(:eddsa, :ed25519)
    {replaced, _} = :crypto.generate_key(:eddsa, :ed25519)

    did_key = fn key ->
      "did:key:z" <> base58(<<0xED, 0x01>> <> key)
    end

    assert {:ok, :replaced} =
             KeyFetcher.audit_anchor_history(
               [
                 %{"operation" => %{"verificationMethods" => %{"opake" => did_key.(replaced)}}},
                 %{"nullified" => true, "operation" => %{"verificationMethods" => %{}}},
                 %{"operation" => %{"verificationMethods" => %{"opake" => did_key.(current)}}}
               ],
               current
             )

    assert {:ok, :not_replaced} =
             KeyFetcher.audit_anchor_history(
               [
                 %{"operation" => %{"verificationMethods" => %{"opake" => did_key.(current)}}},
                 %{"operation" => %{"verificationMethods" => %{}}},
                 %{"operation" => %{"verificationMethods" => %{"opake" => did_key.(current)}}}
               ],
               current
             )

    assert {:error, {:invalid, :plc_audit_history}} =
             KeyFetcher.audit_anchor_history([%{"verificationMethods" => %{}}], current)

    for malformed <- [
          %{"operation" => %{"verificationMethods" => nil}},
          %{"operation" => %{"verificationMethods" => %{"opake" => nil}}}
        ] do
      assert {:error, {:invalid, :plc_audit_history}} =
               KeyFetcher.audit_anchor_history([malformed], current)
    end

    assert {:ok, :no_history} = KeyFetcher.history_notice("did:web:example.test", [], current)
  end

  test "anchored public-key vocabulary is read from the shared registry" do
    {record, did_doc} = signed_record()

    assert {:error, {:invalid, :public_key_record}} =
             KeyFetcher.validate_record(@did, did_doc, Map.put(record, "x25519Algo", "rot13"))
  end

  defp http_server(requests, response) do
    {:ok, listener} =
      :gen_tcp.listen(0, [:binary, active: false, reuseaddr: true, ip: {127, 0, 0, 1}])

    {:ok, {{127, 0, 0, 1}, port}} = :inet.sockname(listener)
    base_url = "http://127.0.0.1:#{port}"

    server =
      spawn_link(fn ->
        serve_http_requests(listener, requests, response)
      end)

    {base_url, server}
  end

  defp serve_http_requests(listener, 0, _response), do: :gen_tcp.close(listener)

  defp serve_http_requests(listener, remaining, response) do
    {:ok, socket} = :gen_tcp.accept(listener)
    {:ok, request} = :gen_tcp.recv(socket, 0, 5_000)

    {status, payload} =
      case response.("http://127.0.0.1:#{port(listener)}", request) do
        {status, payload} when is_integer(status) -> {status, payload}
        payload -> {200, payload}
      end

    body = Jason.encode!(payload)

    :ok =
      :gen_tcp.send(
        socket,
        [
          "HTTP/1.1 #{status} R\r\ncontent-type: application/did+ld+json\r\ncontent-length: ",
          Integer.to_string(byte_size(body)),
          "\r\nconnection: close\r\n\r\n",
          body
        ]
      )

    :gen_tcp.close(socket)
    serve_http_requests(listener, remaining - 1, response)
  end

  defp port(listener) do
    {:ok, {{127, 0, 0, 1}, port}} = :inet.sockname(listener)
    port
  end
end
