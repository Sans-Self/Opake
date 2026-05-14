defmodule OpakeIndexer.Schemas.Keyring do
  @moduledoc """
  An indexed `app.opake.keyring` record. Stores the per-rotation crypto
  payload — rotation counter, encrypted metadata. The chain head pointer
  for a workspace lives in `keyring_chains`; member rows for the *current*
  head live in `keyring_members`.

  `supersedes_uri` is the back-edge to the prior canonical keyring in the
  same chain. Null for genesis records.

  `workspace_id` is the genesis URI of the chain this record belongs to —
  set during indexing by walking back through `supersedes_uri`. For genesis
  records, `workspace_id = uri`.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key {:uri, :string, autogenerate: false}
  schema "keyrings" do
    field :workspace_id, :string
    field :rotation, :integer, default: 0
    field :encrypted_metadata, :map
    field :supersedes_uri, :string
    field :created_at, :string
    field :modified_at, :string
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(keyring, attrs) do
    keyring
    |> cast(attrs, [
      :uri,
      :workspace_id,
      :rotation,
      :encrypted_metadata,
      :supersedes_uri,
      :created_at,
      :modified_at,
      :indexed_at
    ])
    |> validate_required([:uri, :indexed_at])
  end
end
