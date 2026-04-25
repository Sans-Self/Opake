defmodule OpakeIndexer.Queries.DocumentQueries do
  @moduledoc """
  Document CRUD queries. Documents are upserted on create/update events
  and soft-deleted on delete events.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Document

  @spec upsert_document(map()) :: {:ok, Document.t()} | {:error, Ecto.Changeset.t()}
  def upsert_document(attrs) do
    %Document{}
    |> Document.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:document_uri]},
      conflict_target: :document_uri
    )
  end

  @spec soft_delete_document(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete_document(document_uri, now) do
    from(d in Document, where: d.document_uri == ^document_uri)
    |> Repo.update_all(set: [deleted_at: now])
  end

  @spec list_documents(String.t()) :: [Document.t()]
  def list_documents(keyring_uri) do
    from(d in Document, where: d.keyring_uri == ^keyring_uri and is_nil(d.deleted_at))
    |> Repo.all()
  end

  @doc """
  Resolve the keyring URI for a document. Used by the indexer to enrich
  `documentUpdate` proposal broadcasts with workspace routing info —
  the `app.opake.documentUpdate` lexicon itself carries no `keyring`
  field, so the broadcaster has nowhere to route the event without
  looking up the document. Returns `:not_found` for cabinet documents
  (no workspace) and for documents that haven't been indexed yet (race
  between the proposal arriving and its parent document being indexed).
  """
  @spec keyring_uri_for_document(String.t()) :: {:ok, String.t()} | :not_found
  def keyring_uri_for_document(document_uri) do
    query =
      from(d in Document,
        where: d.document_uri == ^document_uri and not is_nil(d.keyring_uri),
        select: d.keyring_uri
      )

    case Repo.one(query) do
      nil -> :not_found
      uri -> {:ok, uri}
    end
  end
end
