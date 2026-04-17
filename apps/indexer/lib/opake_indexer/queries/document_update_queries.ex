defmodule OpakeIndexer.Queries.DocumentUpdateQueries do
  @moduledoc """
  Document update index queries. Indexed from `app.opake.documentUpdate`
  firehose events. Surfaces pending updates for the document owner to apply.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.DocumentUpdate
  alias OpakeIndexer.Queries.Pagination

  @spec upsert_document_update(map()) :: {:ok, DocumentUpdate.t()} | {:error, Ecto.Changeset.t()}
  def upsert_document_update(attrs) do
    %DocumentUpdate{}
    |> DocumentUpdate.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  @spec delete_document_update(String.t()) :: {non_neg_integer(), nil}
  def delete_document_update(uri) do
    from(du in DocumentUpdate, where: du.uri == ^uri)
    |> Repo.delete_all()
  end

  @spec list_document_updates(String.t(), keyword()) :: {[DocumentUpdate.t()], String.t() | nil}
  def list_document_updates(document_uri, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(du in DocumentUpdate,
        where: du.document_uri == ^document_uri,
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
  All pending document updates for documents in a workspace, ordered by
  indexed_at ascending. Used in workspace sync responses. Only includes
  updates authored by workspace members (via keyring_members join).
  """
  @spec member_document_updates(String.t()) :: [DocumentUpdate.t()]
  def member_document_updates(keyring_uri) do
    alias OpakeIndexer.Schemas.{Document, KeyringMember}

    from(du in DocumentUpdate,
      join: d in Document,
      on: d.document_uri == du.document_uri,
      join: km in KeyringMember,
      on: km.keyring_uri == d.keyring_uri and km.member_did == du.author_did,
      where: d.keyring_uri == ^keyring_uri,
      order_by: [asc: du.indexed_at]
    )
    |> Repo.all()
  end

  @spec list_updates_for_owner(String.t(), keyword()) :: {[DocumentUpdate.t()], String.t() | nil}
  def list_updates_for_owner(owner_did, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    alias OpakeIndexer.Schemas.Document

    query =
      from(du in DocumentUpdate,
        join: wd in Document,
        on: wd.document_uri == du.document_uri,
        where: wd.owner_did == ^owner_did and du.author_did != ^owner_did,
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
end
