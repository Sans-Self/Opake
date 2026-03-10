defmodule OpakeAppview.Queries.KeyringQueries do
  @moduledoc """
  Keyring membership CRUD and queries. Upserts are transactional
  delete-all-then-reinsert to match the "full member list" semantics of
  the `app.opake.keyring` record. `list_keyrings_for_member/3` returns
  distinct keyrings containing a given DID, with cursor-based pagination.
  """

  import Ecto.Query

  alias OpakeAppview.Repo
  alias OpakeAppview.Schemas.KeyringMember
  alias OpakeAppview.Queries.Pagination

  def upsert_keyring(keyring_uri, owner_did, member_dids) do
    now = DateTime.utc_now()

    Repo.transaction(fn ->
      from(km in KeyringMember, where: km.keyring_uri == ^keyring_uri)
      |> Repo.delete_all()

      rows =
        Enum.map(member_dids, fn member_did ->
          %{
            keyring_uri: keyring_uri,
            member_did: member_did,
            owner_did: owner_did,
            indexed_at: now
          }
        end)

      Repo.insert_all(KeyringMember, rows)
    end)
  end

  def delete_keyring(keyring_uri) do
    from(km in KeyringMember, where: km.keyring_uri == ^keyring_uri)
    |> Repo.delete_all()
  end

  def list_keyrings_for_member(member_did, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(km in KeyringMember,
        where: km.member_did == ^member_did,
        distinct: km.keyring_uri,
        order_by: [desc: km.indexed_at, desc: km.keyring_uri],
        select: %{
          uri: km.keyring_uri,
          owner_did: km.owner_did,
          indexed_at: km.indexed_at
        },
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from(km in query,
            where:
              km.indexed_at < ^cursor_time or
                (km.indexed_at == ^cursor_time and km.keyring_uri < ^cursor_uri)
          )

        :none ->
          query
      end

    keyrings = Repo.all(query)
    next_cursor = Pagination.build_next_cursor(keyrings)

    {keyrings, next_cursor}
  end

  def keyring_count do
    from(km in KeyringMember, select: count(km.keyring_uri, :distinct))
    |> Repo.one()
  end
end
