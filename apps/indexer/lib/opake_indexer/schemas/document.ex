defmodule OpakeIndexer.Schemas.Document do
  @moduledoc """
  An indexed `app.opake.document` record. Documents are encrypted files stored
  on a PDS. Workspace documents reference a keyring; cabinet documents don't.
  The indexer indexes these from the firehose for tree/sync queries.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "documents" do
    field :document_uri, :string, primary_key: true
    field :keyring_uri, :string
    field :owner_did, :string
    field :rotation, :integer
    field :encrypted_metadata, :map
    field :encryption, :map
    field :blob_ref, :map
    field :deleted_at, :utc_datetime_usec
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(doc, attrs) do
    doc
    |> cast(attrs, [
      :document_uri,
      :keyring_uri,
      :owner_did,
      :rotation,
      :encrypted_metadata,
      :encryption,
      :blob_ref,
      :deleted_at,
      :indexed_at
    ])
    |> validate_required([:document_uri, :owner_did, :indexed_at])
  end
end
