defmodule OpakeAppview.Queries.DocumentQueries do
  @moduledoc """
  Document CRUD queries. Documents are upserted on create/update events
  and soft-deleted on delete events.
  """

  import Ecto.Query

  alias OpakeAppview.Repo
  alias OpakeAppview.Schemas.Document

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
end
