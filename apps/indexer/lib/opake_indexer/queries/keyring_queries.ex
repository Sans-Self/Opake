defmodule OpakeIndexer.Queries.KeyringQueries do
  @moduledoc """
  Keyring membership CRUD and queries. Upserts are transactional
  delete-all-then-reinsert to match the "full member list" semantics of
  the `app.opake.keyring` record. `list_keyrings_for_member/3` returns
  distinct keyrings containing a given DID, with cursor-based pagination.
  `list_keyrings_full/3` joins the full keyring record data.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.{Keyring, KeyringMember, KeyringUpdate}
  alias OpakeIndexer.Queries.Pagination

  @spec upsert_keyring(String.t(), String.t(), [map()]) :: {:ok, term()} | {:error, term()}
  def upsert_keyring(keyring_uri, owner_did, member_entries) do
    now = DateTime.utc_now()

    Repo.transaction(fn ->
      from(km in KeyringMember, where: km.keyring_uri == ^keyring_uri)
      |> Repo.delete_all()

      rows =
        Enum.map(member_entries, fn entry ->
          %{
            keyring_uri: keyring_uri,
            member_did: entry.did,
            owner_did: owner_did,
            role: entry[:role],
            wrapped_key: entry[:wrapped_key],
            indexed_at: now
          }
        end)

      Repo.insert_all(KeyringMember, rows)
    end)
  end

  @spec upsert_keyring_record(map()) :: {:ok, Keyring.t()} | {:error, Ecto.Changeset.t()}
  def upsert_keyring_record(attrs) do
    %Keyring{
      uri: attrs.uri,
      owner_did: attrs.owner_did,
      rotation: attrs[:rotation] || 0,
      encrypted_metadata: attrs[:encrypted_metadata],
      created_at: attrs[:created_at],
      modified_at: attrs[:modified_at],
      indexed_at: DateTime.utc_now()
    }
    |> Repo.insert(
      on_conflict:
        {:replace, [:rotation, :encrypted_metadata, :created_at, :modified_at, :indexed_at]},
      conflict_target: :uri
    )
  end

  @spec remove_member(String.t(), String.t()) :: {non_neg_integer(), nil}
  def remove_member(keyring_uri, member_did) do
    from(km in KeyringMember,
      where: km.keyring_uri == ^keyring_uri and km.member_did == ^member_did
    )
    |> Repo.delete_all()
  end

  @spec delete_keyring(String.t()) :: :ok
  def delete_keyring(keyring_uri) do
    from(km in KeyringMember, where: km.keyring_uri == ^keyring_uri)
    |> Repo.delete_all()

    from(k in Keyring, where: k.uri == ^keyring_uri)
    |> Repo.delete_all()

    :ok
  end

  @spec list_keyrings_for_member(String.t(), keyword()) :: {[map()], String.t() | nil}
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

  @doc """
  List keyrings with full data (keyring-level + all members) for a given DID.
  Returns `{[{Keyring.t(), [KeyringMember.t()]}], cursor}`.
  """
  @spec list_keyrings_full(String.t(), keyword()) ::
          {[{Keyring.t(), [KeyringMember.t()]}], String.t() | nil}
  def list_keyrings_full(member_did, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    # Step 1: find keyring URIs the DID is a member of
    uri_query =
      from(km in KeyringMember,
        join: k in Keyring,
        on: km.keyring_uri == k.uri,
        where: km.member_did == ^member_did,
        select: k,
        order_by: [desc: k.indexed_at, desc: k.uri],
        limit: ^limit
      )

    uri_query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from([km, k] in uri_query,
            where:
              k.indexed_at < ^cursor_time or
                (k.indexed_at == ^cursor_time and k.uri < ^cursor_uri)
          )

        :none ->
          uri_query
      end

    keyrings = Repo.all(uri_query)
    next_cursor = Pagination.build_next_cursor(keyrings)

    # Step 2: batch-load all members for these keyrings
    uris = Enum.map(keyrings, & &1.uri)

    members_by_uri =
      if uris == [] do
        %{}
      else
        from(km in KeyringMember, where: km.keyring_uri in ^uris)
        |> Repo.all()
        |> Enum.group_by(& &1.keyring_uri)
      end

    pairs = Enum.map(keyrings, fn k -> {k, Map.get(members_by_uri, k.uri, [])} end)

    {pairs, next_cursor}
  end

  @spec is_member?(String.t(), String.t()) :: boolean()
  def is_member?(keyring_uri, did) do
    from(km in KeyringMember,
      where: km.keyring_uri == ^keyring_uri and km.member_did == ^did
    )
    |> Repo.exists?()
  end

  @spec all_member_dids() :: [String.t()]
  def all_member_dids do
    from(km in KeyringMember, select: km.member_did, distinct: true)
    |> Repo.all()
  end

  @spec keyring_count() :: non_neg_integer()
  def keyring_count do
    from(km in KeyringMember, select: count(km.keyring_uri, :distinct))
    |> Repo.one()
  end

  # -- Keyring updates (proposals) --

  @spec upsert_keyring_update(map()) :: {:ok, KeyringUpdate.t()} | {:error, Ecto.Changeset.t()}
  def upsert_keyring_update(attrs) do
    %KeyringUpdate{}
    |> KeyringUpdate.changeset(attrs)
    |> Repo.insert(on_conflict: {:replace_all_except, [:uri]}, conflict_target: :uri)
  end

  @spec delete_keyring_update(String.t()) :: :ok
  def delete_keyring_update(uri) do
    from(ku in KeyringUpdate, where: ku.uri == ^uri) |> Repo.delete_all()
    :ok
  end

  @doc "List keyring updates for a workspace, verified against membership."
  @spec member_keyring_updates(String.t()) :: [KeyringUpdate.t()]
  def member_keyring_updates(keyring_uri) do
    from(ku in KeyringUpdate,
      join: km in KeyringMember,
      on: km.keyring_uri == ku.keyring_uri and km.member_did == ku.author_did,
      where: ku.keyring_uri == ^keyring_uri,
      order_by: [asc: ku.indexed_at]
    )
    |> Repo.all()
  end
end
