defmodule OpakeIndexer.Repo.Migrations.FederationSchema do
  @moduledoc """
  Federation-era schema. Drop-and-recreate from empty — no migration path
  from the proposal-era tables.

  Tables created:
    * cursor — singleton firehose cursor
    * grants — sharing primitives (unchanged conceptually)
    * keyrings — per-rotation/supersede keyring records (one row per chain member)
    * keyring_chains — current head pointer per workspace
    * keyring_members — flattened membership for the current head
    * workspace_roots — current head pointer for each workspace's root directory chain
    * directories — workspace and cabinet directory records
    * documents — workspace and cabinet document records
  """

  use Ecto.Migration

  def change do
    # -- Cursor ---------------------------------------------------------

    create table(:cursor, primary_key: false) do
      add :id, :integer, primary_key: true
      add :time_us, :bigint, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    create constraint(:cursor, :cursor_singleton, check: "id = 1")

    # -- Grants ---------------------------------------------------------

    create table(:grants, primary_key: false) do
      add :uri, :text, primary_key: true
      add :author_did, :text, null: false
      add :recipient_did, :text, null: false
      add :document_uri, :text, null: false
      add :created_at, :text, null: false
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:grants, [:recipient_did, :indexed_at])
    create index(:grants, [:author_did, :indexed_at])
    create index(:grants, [:document_uri])

    # -- Keyrings (chain members) --------------------------------------

    create table(:keyrings, primary_key: false) do
      add :uri, :text, primary_key: true
      add :workspace_id, :text
      add :rotation, :integer, default: 0, null: false
      add :encrypted_metadata, :map
      add :supersedes_uri, :text
      add :created_at, :text
      add :modified_at, :text
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:keyrings, [:workspace_id, :indexed_at])
    create index(:keyrings, [:supersedes_uri])

    # -- Keyring chain heads -------------------------------------------

    create table(:keyring_chains, primary_key: false) do
      add :workspace_id, :text, primary_key: true
      add :head_uri, :text, null: false
      add :head_cid, :text, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    # -- Keyring members (current head only) ---------------------------

    create table(:keyring_members, primary_key: false) do
      add :workspace_id, :text, primary_key: true
      add :member_did, :text, primary_key: true
      add :role, :text
      add :wrapped_key, :map
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:keyring_members, [:member_did, :indexed_at])

    # -- Workspace root pointers ---------------------------------------

    create table(:workspace_roots, primary_key: false) do
      add :workspace_id, :text, primary_key: true
      add :head_uri, :text, null: false
      add :head_cid, :text, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    # -- Directories ---------------------------------------------------

    create table(:directories, primary_key: false) do
      add :uri, :text, primary_key: true
      add :workspace_id, :text
      add :chain_genesis_uri, :text
      add :author_did, :text, null: false
      add :entries_json, {:array, :map}, default: []
      add :encrypted_metadata, :map
      add :key_wrapping, :map
      add :supersedes_uri, :text
      add :modified_at, :text
      add :deleted_at, :utc_datetime_usec
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:directories, [:workspace_id, :indexed_at])
    create index(:directories, [:chain_genesis_uri])
    create index(:directories, [:author_did, :indexed_at],
             where: "workspace_id IS NULL",
             name: :directories_cabinet_idx
           )
    create index(:directories, [:supersedes_uri])

    # -- Documents -----------------------------------------------------

    create table(:documents, primary_key: false) do
      add :uri, :text, primary_key: true
      add :workspace_id, :text
      add :author_did, :text, null: false
      add :rotation, :integer
      add :encrypted_metadata, :map
      add :encryption, :map
      add :blob_ref, :map
      add :supersedes_uri, :text
      add :modified_at, :text
      add :deleted_at, :utc_datetime_usec
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:documents, [:workspace_id, :indexed_at])
    create index(:documents, [:author_did, :indexed_at],
             where: "workspace_id IS NULL",
             name: :documents_cabinet_idx
           )
    create index(:documents, [:supersedes_uri])
  end
end
