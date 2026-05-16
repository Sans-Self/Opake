defmodule OpakeIndexer.Schemas.ChainHead do
  @moduledoc """
  Current head pointer per workspace chain.

  One row per `(workspace_id, kind)` pair. Two kinds are tracked today:

    * `"keyring"` — the workspace keyring chain (membership, rotation,
      rename, relocation).
    * `"workspace_root"` — the workspace's root directory chain.

  The dispatch updates this row in the same transaction as the record
  insert that produced the new head. Compare-and-set (matching the prior
  `head_uri`) detects chain forks.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "chain_heads" do
    field :workspace_id, :string, primary_key: true
    field :kind, :string, primary_key: true
    field :head_uri, :string
    field :head_cid, :string
    field :updated_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(head, attrs) do
    head
    |> cast(attrs, [:workspace_id, :kind, :head_uri, :head_cid, :updated_at])
    |> validate_required([:workspace_id, :kind, :head_uri, :head_cid, :updated_at])
    |> validate_inclusion(:kind, ["keyring", "workspace_root"])
  end
end
