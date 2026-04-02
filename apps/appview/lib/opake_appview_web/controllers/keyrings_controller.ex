defmodule OpakeAppviewWeb.KeyringsController do
  @moduledoc """
  Returns full keyring records for keyrings where the authenticated DID is a
  member. Joins `keyring_members` (membership + wrapped keys) with `keyrings`
  (rotation, encrypted metadata) so clients get everything in one call.
  """

  use OpakeAppviewWeb, :controller

  alias OpakeAppview.Queries.KeyringQueries
  import OpakeAppviewWeb.PaginationHelpers

  @spec index(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def index(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {pairs, next_cursor} = KeyringQueries.list_keyrings_full(did, limit: limit, cursor: cursor)

      response =
        %{
          keyrings:
            Enum.map(pairs, fn {k, members} ->
              %{
                uri: k.uri,
                owner_did: k.owner_did,
                rotation: k.rotation,
                members:
                  Enum.map(members, fn m ->
                    %{
                      "wrappedKey" => m.wrapped_key,
                      "role" => m.role
                    }
                  end),
                encrypted_metadata: k.encrypted_metadata,
                created_at: k.created_at,
                indexed_at: DateTime.to_iso8601(k.indexed_at)
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
