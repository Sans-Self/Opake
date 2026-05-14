defmodule OpakeIndexer.Schemas.KeyringMember do
  @moduledoc """
  A denormalized membership row reflecting the *current* head keyring's
  members. Replaced wholesale on every keyring supersede via
  `KeyringQueries.upsert_keyring_members/3` (delete-all-then-insert in a
  transaction) — matches the "full member list" semantics of the keyring
  record.

  Keyed by `(workspace_id, member_did)` so cross-rotation lookups are
  stable: the same DID stays a member across rotations of the same
  workspace, even though each rotation produces a new keyring URI.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "keyring_members" do
    field :workspace_id, :string, primary_key: true
    field :member_did, :string, primary_key: true
    field :role, :string
    field :wrapped_key, :map
    field :indexed_at, :utc_datetime_usec
  end

  def changeset(member, attrs) do
    member
    |> cast(attrs, [:workspace_id, :member_did, :role, :wrapped_key, :indexed_at])
    |> validate_required([:workspace_id, :member_did, :indexed_at])
  end
end
