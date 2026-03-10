defmodule OpakeAppviewWeb.InboxController do
  @moduledoc """
  Returns incoming grants for the authenticated DID. The `?did=` parameter
  must match the authenticated DID (enforced by the auth plug's scope check).
  Supports cursor-based pagination with configurable limit (1-100, default 50).
  """

  use OpakeAppviewWeb, :controller

  alias OpakeAppview.Queries.GrantQueries
  import OpakeAppviewWeb.PaginationHelpers

  def index(conn, params) do
    with {:ok, did} <- require_did(params),
         {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {grants, next_cursor} = GrantQueries.list_inbox(did, limit: limit, cursor: cursor)

      response =
        %{
          grants:
            Enum.map(grants, fn g ->
              %{
                uri: g.uri,
                ownerDid: g.owner_did,
                documentUri: g.document_uri,
                createdAt: g.created_at
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
