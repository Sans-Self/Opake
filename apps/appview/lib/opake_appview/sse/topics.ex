defmodule OpakeAppview.SSE.Topics do
  @moduledoc """
  Canonical PubSub topic construction for SSE event routing.

  Two topic tiers:
  - `"did:<did>"` — personal events (cabinet, grants, owned keyrings)
  - `"keyring:<uri>"` — workspace-scoped events (dirs/docs/proposals)
  """

  @spec personal(String.t()) :: String.t()
  def personal(did), do: "did:#{did}"

  @spec workspace(String.t()) :: String.t()
  def workspace(keyring_uri), do: "keyring:#{keyring_uri}"
end
