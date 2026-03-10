defmodule OpakeAppview.Queries.GrantQueries do
  @moduledoc """
  Grant CRUD and inbox queries. Grants are upserted on create/update events
  and deleted on delete events. `list_inbox/3` returns grants for a recipient
  DID with cursor-based pagination (ordered by indexed_at DESC, uri DESC).
  """

  import Ecto.Query

  alias OpakeAppview.Repo
  alias OpakeAppview.Schemas.Grant
  alias OpakeAppview.Queries.Pagination

  def upsert_grant(attrs) do
    %Grant{}
    |> Grant.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  def delete_grant(uri) do
    from(g in Grant, where: g.uri == ^uri)
    |> Repo.delete_all()
  end

  def list_inbox(recipient_did, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(g in Grant,
        where: g.recipient_did == ^recipient_did,
        order_by: [desc: g.indexed_at, desc: g.uri],
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from(g in query,
            where:
              g.indexed_at < ^cursor_time or
                (g.indexed_at == ^cursor_time and g.uri < ^cursor_uri)
          )

        :none ->
          query
      end

    grants = Repo.all(query)
    next_cursor = Pagination.build_next_cursor(grants)

    {grants, next_cursor}
  end

  def grant_count do
    Repo.aggregate(Grant, :count)
  end
end
