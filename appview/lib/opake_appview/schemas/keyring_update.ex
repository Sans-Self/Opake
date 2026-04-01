defmodule OpakeAppview.Schemas.KeyringUpdate do
  @moduledoc "A proposed change to a workspace keyring, written by a member."
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:uri, :string, autogenerate: false}
  schema "keyring_updates" do
    field :keyring_uri, :string
    field :author_did, :string
    field :action_type, :string
    field :member_did, :string
    field :member_public_key, :binary
    field :role, :string
    field :encrypted_metadata, :map
    field :indexed_at, :utc_datetime_usec
  end

  @type t :: %__MODULE__{}

  @cast_fields [
    :uri,
    :keyring_uri,
    :author_did,
    :action_type,
    :member_did,
    :member_public_key,
    :role,
    :encrypted_metadata,
    :indexed_at
  ]

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(update, attrs) do
    update
    |> cast(attrs, @cast_fields)
    |> validate_required([:uri, :keyring_uri, :author_did, :action_type, :indexed_at])
  end
end
