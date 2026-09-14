defmodule OpakeIndexer.Auth.KeyFetcherBehaviour do
  @moduledoc """
  Behaviour for resolving a DID to a complete authentication decision: the
  32-byte Ed25519 signing public key, whether the account is verified against
  a DID-document `#opake` anchor, and what the anchor's replacement history
  says.
  The real implementation hits the network; tests use a Mox mock.
  """

  @typedoc """
  What the DID method's operation history says about the `#opake` anchor:
  read and unchanged, read and replaced at some point, a method that offers
  no history to read, or a history the transport could not deliver. An
  unreadable history never downgrades a verified account — it is reported as
  unknown rather than as an absence of replacement.
  """
  @type anchor_history :: :not_replaced | :replaced | :no_history | :unavailable

  @type decision :: %{
          key: binary(),
          verified: boolean(),
          anchor_history: anchor_history()
        }

  @callback fetch_authentication_decision(did :: String.t()) ::
              {:ok, decision()} | {:error, term()}
end
