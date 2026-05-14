defmodule OpakeIndexerWeb.KeyringsController do
  @moduledoc """
  Returns the current head of each workspace the authenticated DID is a
  member of, joined with that head's member list (so clients receive
  everything they need in one call).
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.KeyringQueries
  import OpakeIndexerWeb.PaginationHelpers

  @spec index(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def index(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {pairs, next_cursor} = KeyringQueries.list_workspaces_full(did, limit: limit, cursor: cursor)

      response =
        %{
          workspaces:
            Enum.map(pairs, fn {k, members} ->
              %{
                workspace_id: k.workspace_id,
                head_uri: k.uri,
                rotation: k.rotation,
                supersedes_uri: k.supersedes_uri,
                members:
                  Enum.map(members, fn m ->
                    %{
                      "wrappedKey" => m.wrapped_key,
                      "role" => m.role
                    }
                  end),
                encrypted_metadata: k.encrypted_metadata,
                created_at: k.created_at,
                modified_at: k.modified_at,
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
