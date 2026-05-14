defmodule OpakeIndexer.Schemas.Directory do
  @moduledoc """
  An indexed `app.opake.directory` record. Directories organize workspace
  trees as supersede chains; cabinet directories are non-chained and
  carry `workspace_id = nil`.

  `entries_json` is `[{target: at-uri, target_cid: string}]` — JSONB so
  Postgres can index into it cheaply if we ever need to.

  `supersedes_uri` is the back-edge. `workspace_id` is denormalized from
  the keyring URI's chain (set during indexing). `chain_genesis_uri` is
  the URI of the first record in *this directory's* chain — for genesis
  records it equals `uri`; for supersedes it's inherited from the prior
  record. Used by chain dispatch to detect whether this directory
  participates in the workspace root chain.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @type t :: %__MODULE__{}

  @primary_key false
  schema "directories" do
    field :uri, :string, primary_key: true
    field :workspace_id, :string
    field :chain_genesis_uri, :string
    field :author_did, :string
    field :entries_json, {:array, :map}, default: []
    field :encrypted_metadata, :map
    field :key_wrapping, :map
    field :supersedes_uri, :string
    field :modified_at, :string
    field :deleted_at, :utc_datetime_usec
    field :indexed_at, :utc_datetime_usec
  end

  @spec changeset(t(), map()) :: Ecto.Changeset.t()
  def changeset(dir, attrs) do
    dir
    |> cast(attrs, [
      :uri,
      :workspace_id,
      :chain_genesis_uri,
      :author_did,
      :entries_json,
      :encrypted_metadata,
      :key_wrapping,
      :supersedes_uri,
      :modified_at,
      :deleted_at,
      :indexed_at
    ])
    |> validate_required([:uri, :author_did, :indexed_at])
  end
end
