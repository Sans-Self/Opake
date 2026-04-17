defmodule OpakeIndexer.Schemas.DocumentUpdate do
  @moduledoc """
  A proposed update to a document in a workspace. Written by an editor to
  their own PDS, indexed here for the document owner to discover pending
  updates. Deleted by the editor after the owner applies the update.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:uri, :string, autogenerate: false}
  schema "document_updates" do
    field :document_uri, :string
    field :author_did, :string
    field :supersedes_uri, :string
    field :indexed_at, :utc_datetime_usec
  end

  def changeset(update, attrs) do
    update
    |> cast(attrs, [:uri, :document_uri, :author_did, :supersedes_uri, :indexed_at])
    |> validate_required([:uri, :document_uri, :author_did, :indexed_at])
  end
end
