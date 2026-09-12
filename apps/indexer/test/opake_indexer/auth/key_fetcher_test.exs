defmodule OpakeIndexer.Auth.KeyFetcherTest do
  use ExUnit.Case, async: true

  alias OpakeIndexer.Auth.KeyFetcher

  @did "did:plc:alice"

  defp document(methods \\ []), do: %{"id" => @did, "verificationMethod" => methods}

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
end
