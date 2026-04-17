defmodule OpakeIndexer.Queries.DirectoryQueries do
  @moduledoc """
  Directory CRUD and tree/sync queries. Directories are upserted on
  create/update events and soft-deleted on delete events. Supports
  workspace tree (by keyring_uri) and cabinet tree (by owner_did, no keyring).
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.{Directory, DirectoryUpdate, Document}
  alias OpakeIndexer.Queries.Pagination

  @spec upsert_directory(map()) :: {:ok, Directory.t()} | {:error, Ecto.Changeset.t()}
  def upsert_directory(attrs) do
    %Directory{}
    |> Directory.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:directory_uri]},
      conflict_target: :directory_uri
    )
  end

  @spec soft_delete_directory(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete_directory(directory_uri, now) do
    from(d in Directory, where: d.directory_uri == ^directory_uri)
    |> Repo.update_all(set: [deleted_at: now, entries: []])
  end

  @spec workspace_tree(String.t()) :: {[Directory.t()], [Document.t()]}
  def workspace_tree(keyring_uri) do
    dirs =
      from(d in Directory, where: d.keyring_uri == ^keyring_uri and is_nil(d.deleted_at))
      |> Repo.all()

    docs =
      from(d in Document, where: d.keyring_uri == ^keyring_uri and is_nil(d.deleted_at))
      |> Repo.all()

    {dirs, docs}
  end

  @spec cabinet_tree(String.t()) :: {[Directory.t()], [Document.t()]}
  def cabinet_tree(owner_did) do
    dirs =
      from(d in Directory,
        where: d.owner_did == ^owner_did and is_nil(d.keyring_uri) and is_nil(d.deleted_at)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where: d.owner_did == ^owner_did and is_nil(d.keyring_uri) and is_nil(d.deleted_at)
      )
      |> Repo.all()

    {dirs, docs}
  end

  @spec workspace_changes_since(String.t(), DateTime.t()) :: {[Directory.t()], [Document.t()]}
  def workspace_changes_since(keyring_uri, since) do
    dirs =
      from(d in Directory,
        where:
          d.keyring_uri == ^keyring_uri and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where:
          d.keyring_uri == ^keyring_uri and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    {dirs, docs}
  end

  @spec cabinet_changes_since(String.t(), DateTime.t()) :: {[Directory.t()], [Document.t()]}
  def cabinet_changes_since(owner_did, since) do
    dirs =
      from(d in Directory,
        where:
          d.owner_did == ^owner_did and is_nil(d.keyring_uri) and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where:
          d.owner_did == ^owner_did and is_nil(d.keyring_uri) and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    {dirs, docs}
  end

  # -- Directory updates (audit log) --

  @spec upsert_directory_update(map()) ::
          {:ok, DirectoryUpdate.t()} | {:error, Ecto.Changeset.t()}
  def upsert_directory_update(attrs) do
    %DirectoryUpdate{}
    |> DirectoryUpdate.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  @spec delete_directory_update(String.t()) :: {non_neg_integer(), nil}
  def delete_directory_update(uri) do
    from(du in DirectoryUpdate, where: du.uri == ^uri)
    |> Repo.delete_all()
  end

  @spec list_directory_updates(String.t(), keyword()) :: {[DirectoryUpdate.t()], String.t() | nil}
  def list_directory_updates(keyring_uri, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(du in DirectoryUpdate,
        where: du.keyring_uri == ^keyring_uri,
        order_by: [desc: du.indexed_at, desc: du.uri],
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from(du in query,
            where:
              du.indexed_at < ^cursor_time or
                (du.indexed_at == ^cursor_time and du.uri < ^cursor_uri)
          )

        :none ->
          query
      end

    updates = Repo.all(query)
    next_cursor = Pagination.build_next_cursor(updates)

    {updates, next_cursor}
  end

  @doc """
  List directory updates for a workspace, verified against membership.

  Only returns updates whose `author_did` is a current member of the keyring.
  Used by the tree sync response so clients can trust the proposals.
  """
  @spec member_directory_updates(String.t()) :: [DirectoryUpdate.t()]
  def member_directory_updates(keyring_uri) do
    alias OpakeIndexer.Schemas.KeyringMember

    from(du in DirectoryUpdate,
      join: km in KeyringMember,
      on: km.keyring_uri == du.keyring_uri and km.member_did == du.author_did,
      where: du.keyring_uri == ^keyring_uri,
      order_by: [asc: du.indexed_at]
    )
    |> Repo.all()
  end

  # -- Tombstone cleanup --

  @spec purge_tombstones(DateTime.t()) :: {non_neg_integer(), non_neg_integer()}
  def purge_tombstones(before) do
    {dir_count, _} =
      from(d in Directory, where: not is_nil(d.deleted_at) and d.deleted_at < ^before)
      |> Repo.delete_all()

    {doc_count, _} =
      from(d in Document, where: not is_nil(d.deleted_at) and d.deleted_at < ^before)
      |> Repo.delete_all()

    {dir_count, doc_count}
  end
end
