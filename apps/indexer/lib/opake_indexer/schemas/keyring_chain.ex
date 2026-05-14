defmodule OpakeIndexer.Schemas.KeyringChain do
  @moduledoc """
  The current head of a workspace's keyring supersede chain. One row per
  workspace (keyed by `workspace_id`, which is the genesis keyring URI).

  Advanced on every keyring upsert whose `supersedes_uri` matches the
  current `head_uri`. Forks (supersedes pointing at non-head) are
  detected here — see `OpakeIndexer.Firehose` dispatch.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key {:workspace_id, :string, autogenerate: false}
  schema "keyring_chains" do
    field :head_uri, :string
    field :head_cid, :string
    field :updated_at, :utc_datetime_usec
  end

  def changeset(chain, attrs) do
    chain
    |> cast(attrs, [:workspace_id, :head_uri, :head_cid, :updated_at])
    |> validate_required([:workspace_id, :head_uri, :head_cid, :updated_at])
  end
end
