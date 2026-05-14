defmodule OpakeIndexerWeb.InboxController do
  @moduledoc """
  Returns incoming grants for the authenticated DID.
  Supports cursor-based pagination with configurable limit (1-100, default 50).
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.GrantQueries
  import OpakeIndexerWeb.PaginationHelpers

  def index(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {grants, next_cursor} = GrantQueries.list_inbox(did, limit: limit, cursor: cursor)

      response =
        %{
          grants:
            Enum.map(grants, fn g ->
              %{
                uri: g.uri,
                author_did: g.author_did,
                document_uri: g.document_uri,
                created_at: g.created_at
              }
            end)
        }
        |> maybe_put_cursor(next_cursor)

      json(conn, response)
    else
      {:error, message} ->
        conn
        |> put_status(400)
        |> json(%{error: message})
    end
  end
end
