defmodule OpakeIndexerWeb.InboxController do
  @moduledoc """
  Returns incoming grants for the authenticated DID, in the envelope
  shape `[{record, indexedAt, deletedAt?}, ...]`. Supports cursor-based
  pagination (1-100 per page, default 50).
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.RecordQueries
  import OpakeIndexerWeb.PaginationHelpers
  import OpakeIndexerWeb.TreeHelpers, only: [envelope: 1]

  def index(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      {grants, next_cursor} = RecordQueries.inbox(did, limit: limit, cursor: cursor)

      response =
        %{grants: Enum.map(grants, &envelope/1)}
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
