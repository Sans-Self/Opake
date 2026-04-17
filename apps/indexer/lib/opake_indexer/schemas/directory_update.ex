defmodule OpakeIndexer.Schemas.DirectoryUpdate do
  @moduledoc """
  A proposed change to a workspace directory. Written by a member to their own
  PDS, indexed here so workspace members can discover pending directory actions
  (add/remove entries, rename, etc.).
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:uri, :string, autogenerate: false}
  schema "directory_updates" do
    field :keyring_uri, :string
    field :author_did, :string
    field :action_type, :string
    field :directory_uri, :string
    field :entry_uri, :string
    field :encrypted_metadata, :map
    field :source_directory_uri, :string
    field :target_directory_uri, :string
    field :parent_directory_uri, :string
    field :indexed_at, :utc_datetime_usec
  end

  @cast_fields [
    :uri,
    :keyring_uri,
    :author_did,
    :action_type,
    :directory_uri,
    :entry_uri,
    :encrypted_metadata,
    :source_directory_uri,
    :target_directory_uri,
    :parent_directory_uri,
    :indexed_at
  ]

  def changeset(update, attrs) do
    update
    |> cast(attrs, @cast_fields)
    |> validate_required([:uri, :keyring_uri, :author_did, :action_type, :indexed_at])
  end
end
