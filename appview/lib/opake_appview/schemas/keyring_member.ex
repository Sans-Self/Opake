defmodule OpakeAppview.Schemas.KeyringMember do
  @moduledoc """
  A denormalized (keyring_uri, member_did) row. Keyrings group encrypted
  document keys for a set of members. The appview flattens the members array
  into individual rows so it can efficiently answer "which keyrings include
  this DID?" via the `/api/keyrings` endpoint. Upserts delete-and-reinsert
  all members atomically.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key false
  schema "keyring_members" do
    field :keyring_uri, :string, primary_key: true
    field :member_did, :string, primary_key: true
    field :owner_did, :string
    field :indexed_at, :utc_datetime_usec
  end

  def changeset(member, attrs) do
    member
    |> cast(attrs, [:keyring_uri, :member_did, :owner_did, :indexed_at])
    |> validate_required([:keyring_uri, :member_did, :owner_did, :indexed_at])
  end
end
