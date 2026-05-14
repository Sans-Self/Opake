defmodule OpakeIndexer.Queries.DocumentQueries do
  @moduledoc """
  Document CRUD queries. Documents are upserted on create/update events
  and soft-deleted on delete events. Documents don't drive chain heads —
  the parent directory's listing entry is the canonical pointer — but they
  carry `supersedes_uri` as a history annotation.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Document

  @spec upsert_document(map()) :: {:ok, Document.t()} | {:error, Ecto.Changeset.t()}
  def upsert_document(attrs) do
    %Document{}
    |> Document.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  @spec soft_delete_document(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete_document(uri, now) do
    from(d in Document, where: d.uri == ^uri)
    |> Repo.update_all(set: [deleted_at: now])
  end

  @spec list_documents(String.t()) :: [Document.t()]
  def list_documents(workspace_id) do
    from(d in Document, where: d.workspace_id == ^workspace_id and is_nil(d.deleted_at))
    |> Repo.all()
  end
end
