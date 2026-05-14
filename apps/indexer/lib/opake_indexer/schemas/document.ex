defmodule OpakeIndexer.Schemas.Document do
  @moduledoc """
  An indexed `app.opake.document` record. Workspace documents reference a
  keyring via `keyringRef`; cabinet documents have `workspace_id = nil`.

  `supersedes_uri` is a history annotation — documents don't drive chain
  heads (the parent directory's listing entry is the canonical pointer).
  Tracked so clients can show lineage if they want.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "documents" do
    field :uri, :string, primary_key: true
    field :workspace_id, :string
    field :author_did, :string
    field :rotation, :integer
    field :encrypted_metadata, :map
    field :encryption, :map
    field :blob_ref, :map
    field :supersedes_uri, :string
    field :modified_at, :string
    field :deleted_at, :utc_datetime_usec
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(doc, attrs) do
    doc
    |> cast(attrs, [
      :uri,
      :workspace_id,
      :author_did,
      :rotation,
      :encrypted_metadata,
      :encryption,
      :blob_ref,
      :supersedes_uri,
      :modified_at,
      :deleted_at,
      :indexed_at
    ])
    |> validate_required([:uri, :author_did, :indexed_at])
  end
end
