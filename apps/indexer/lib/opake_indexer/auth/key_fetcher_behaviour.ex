defmodule OpakeIndexer.Auth.KeyFetcherBehaviour do
  @moduledoc """
  Behaviour for resolving a DID to a 32-byte Ed25519 signing public key.
  The real implementation hits the network; tests use a Mox mock.
  """

  @callback fetch_signing_key(did :: String.t()) :: {:ok, binary()} | {:error, String.t()}
end
