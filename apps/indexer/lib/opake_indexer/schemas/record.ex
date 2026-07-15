defmodule OpakeIndexer.Schemas.Record do
  @moduledoc """
  An indexed `at.opake.*` record.

  `record_jsonb` is the verbatim on-PDS record JSON (camelCase, no field
  renaming). Structural columns are projections of the JSON used for
  index-driven lookups — every projection must be a pure function of
  `record_jsonb` at insert time. The firehose dispatch is the only
  writer; drift is prevented by keeping that surface tiny.

  Soft delete: `deleted_at` is set on tombstone; rows are never removed
  by the dispatch path. Chain rollback on head delete rolls `chain_heads`
  back to the predecessor; the deleted record's row stays put with a
  tombstone so downstream readers see one consistent history.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "records" do
    field :uri, :string, primary_key: true
    field :collection, :string
    field :author_did, :string
    field :workspace_id, :string
    field :supersedes_uri, :string
    field :is_workspace_root, :boolean, default: false
    field :cid, :string
    field :indexed_at, :utc_datetime_usec
    field :updated_at, :utc_datetime_usec
    field :deleted_at, :utc_datetime_usec
    field :record_jsonb, :map
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(record, attrs) do
    record
    |> cast(attrs, [
      :uri,
      :collection,
      :author_did,
      :workspace_id,
      :supersedes_uri,
      :is_workspace_root,
      :cid,
      :indexed_at,
      :updated_at,
      :deleted_at,
      :record_jsonb
    ])
    |> validate_required([
      :uri,
      :collection,
      :author_did,
      :cid,
      :indexed_at,
      :updated_at,
      :record_jsonb
    ])
  end
end
