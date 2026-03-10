defmodule OpakeAppviewWeb.KeyringsController do
  @moduledoc """
  Returns keyrings that include the authenticated DID as a member. Used by
  clients to discover which keyrings they need to fetch wrapped keys from.
  Same pagination pattern as the inbox endpoint.
  """

  use OpakeAppviewWeb, :controller

  alias OpakeAppview.Queries.KeyringQueries
  import OpakeAppviewWeb.PaginationHelpers

  def index(conn, params) do
    with {:ok, did} <- require_did(params),
         {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {keyrings, next_cursor} = KeyringQueries.list_keyrings_for_member(did, limit: limit, cursor: cursor)

      response =
        %{
          keyrings:
            Enum.map(keyrings, fn k ->
              %{
                uri: k.uri,
                ownerDid: k.owner_did,
                indexedAt: DateTime.to_iso8601(k.indexed_at)
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
