defmodule OpakeIndexer.Queries.DirectoryQueries do
  @moduledoc """
  Directory CRUD and tree queries. Directories are upserted on create/update
  events and soft-deleted on delete events. Workspace directories carry a
  `workspace_id`; cabinet directories don't (cabinet trees scope by
  `author_did` with `workspace_id IS NULL`).

  Chain head bookkeeping for workspace root directories lives in
  `OpakeIndexer.Queries.WorkspaceRootQueries` — this module only handles
  the per-record store.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.{Directory, Document}

  @spec upsert_directory(map()) :: {:ok, Directory.t()} | {:error, Ecto.Changeset.t()}
  def upsert_directory(attrs) do
    %Directory{}
    |> Directory.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  @spec soft_delete_directory(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete_directory(uri, now) do
    from(d in Directory, where: d.uri == ^uri)
    |> Repo.update_all(set: [deleted_at: now, entries_json: []])
  end

  @doc "Look up a directory by URI. Used by chain dispatch for genesis-URI inheritance."
  @spec lookup(String.t()) :: Directory.t() | nil
  def lookup(uri), do: Repo.get(Directory, uri)

  @spec workspace_tree(String.t()) :: {[Directory.t()], [Document.t()]}
  def workspace_tree(workspace_id) do
    dirs =
      from(d in Directory,
        where: d.workspace_id == ^workspace_id and is_nil(d.deleted_at)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where: d.workspace_id == ^workspace_id and is_nil(d.deleted_at)
      )
      |> Repo.all()

    {dirs, docs}
  end

  @spec cabinet_tree(String.t()) :: {[Directory.t()], [Document.t()]}
  def cabinet_tree(author_did) do
    dirs =
      from(d in Directory,
        where:
          d.author_did == ^author_did and is_nil(d.workspace_id) and is_nil(d.deleted_at)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where:
          d.author_did == ^author_did and is_nil(d.workspace_id) and is_nil(d.deleted_at)
      )
      |> Repo.all()

    {dirs, docs}
  end

  @spec workspace_changes_since(String.t(), DateTime.t()) :: {[Directory.t()], [Document.t()]}
  def workspace_changes_since(workspace_id, since) do
    dirs =
      from(d in Directory,
        where:
          d.workspace_id == ^workspace_id and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where:
          d.workspace_id == ^workspace_id and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    {dirs, docs}
  end

  @spec cabinet_changes_since(String.t(), DateTime.t()) :: {[Directory.t()], [Document.t()]}
  def cabinet_changes_since(author_did, since) do
    dirs =
      from(d in Directory,
        where:
          d.author_did == ^author_did and is_nil(d.workspace_id) and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    docs =
      from(d in Document,
        where:
          d.author_did == ^author_did and is_nil(d.workspace_id) and
            (d.indexed_at > ^since or d.deleted_at > ^since)
      )
      |> Repo.all()

    {dirs, docs}
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
