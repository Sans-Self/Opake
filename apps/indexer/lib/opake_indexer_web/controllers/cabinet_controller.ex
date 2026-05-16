defmodule OpakeIndexerWeb.CabinetController do
  @moduledoc """
  Cabinet endpoints. Returns the authenticated user's personal (non-workspace)
  directories and documents. No keyring membership check — cabinet data belongs
  to the authenticated DID.
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.RecordQueries
  import OpakeIndexerWeb.TreeHelpers

  @spec snapshot(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def snapshot(conn, _params) do
    did = conn.assigns.authenticated_did
    {directories, documents} = RecordQueries.cabinet_tree(did)
    server_time = DateTime.utc_now()
    json(conn, format_tree_response(directories, documents, server_time))
  end

  @spec sync(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def sync(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, since} <- parse_since(params) do
      {directories, documents} = RecordQueries.cabinet_changes_since(did, since)
      server_time = DateTime.utc_now()
      json(conn, format_tree_response(directories, documents, server_time))
    else
      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end
end
