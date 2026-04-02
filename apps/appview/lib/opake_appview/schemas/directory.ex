defmodule OpakeAppview.Schemas.Directory do
  @moduledoc """
  An indexed `app.opake.directory` record. Directories organize documents within
  workspaces or cabinets. Cabinet directories have no keyring_uri. The appview
  indexes these from the firehose for tree/sync queries.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "directories" do
    field :directory_uri, :string, primary_key: true
    field :keyring_uri, :string
    field :owner_did, :string
    field :entries, {:array, :string}, default: []
    field :encrypted_metadata, :map
    field :key_wrapping, :map
    field :deleted_at, :utc_datetime_usec
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(dir, attrs) do
    dir
    |> cast(attrs, [
      :directory_uri,
      :keyring_uri,
      :owner_did,
      :entries,
      :encrypted_metadata,
      :key_wrapping,
      :deleted_at,
      :indexed_at
    ])
    |> validate_required([:directory_uri, :owner_did, :indexed_at])
  end
end
