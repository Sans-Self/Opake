defmodule OpakeIndexerWeb.KeyringsController do
  @moduledoc """
  Returns the current keyring head envelope for every workspace the
  authenticated DID is a member of. Each envelope is the verbatim
  on-PDS record JSON (camelCase, members array intact) plus indexer
  metadata.

  Pagination is currently not exposed — the member-workspace count is
  bounded by atproto practicalities (a few hundred at most). If real
  workloads grow beyond that, add keyset pagination on
  `(chain_heads.workspace_id, indexed_at)`.
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.RecordQueries
  import OpakeIndexerWeb.TreeHelpers, only: [envelope: 1]

  @spec index(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def index(conn, _params) do
    did = conn.assigns.authenticated_did
    keyrings = RecordQueries.workspaces_for_member(did)
    json(conn, %{workspaces: Enum.map(keyrings, &envelope/1)})
  end
end
