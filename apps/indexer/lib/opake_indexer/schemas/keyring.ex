defmodule OpakeIndexer.Schemas.Keyring do
  @moduledoc """
  An indexed `app.opake.keyring` record. Stores keyring-level data —
  rotation counter, encrypted metadata — so the `/api/keyrings` endpoint
  can serve complete records without clients needing raw XRPC calls.
  Per-member data (wrapped keys, roles) lives in `keyring_members`.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key {:uri, :string, autogenerate: false}
  schema "keyrings" do
    field :owner_did, :string
    field :rotation, :integer, default: 0
    field :encrypted_metadata, :map
    field :created_at, :string
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(keyring, attrs) do
    keyring
    |> cast(attrs, [:uri, :owner_did, :rotation, :encrypted_metadata, :created_at, :indexed_at])
    |> validate_required([:uri, :owner_did, :indexed_at])
  end
end
