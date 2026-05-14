defmodule OpakeIndexer.Schemas.WorkspaceRoot do
  @moduledoc """
  The current head of a workspace's root directory chain. One row per
  workspace. Subtree heads aren't tracked here — clients walk listing
  entries down from this root, and the entries point at current heads
  because cascade rebuilds the full path on every leaf write.

  Stale listing entries (mid-cascade, post-fork) are resolved by walking
  `directories.supersedes_uri` forward.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key {:workspace_id, :string, autogenerate: false}
  schema "workspace_roots" do
    field :head_uri, :string
    field :head_cid, :string
    field :updated_at, :utc_datetime_usec
  end

  def changeset(root, attrs) do
    root
    |> cast(attrs, [:workspace_id, :head_uri, :head_cid, :updated_at])
    |> validate_required([:workspace_id, :head_uri, :head_cid, :updated_at])
  end
end
