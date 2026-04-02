defmodule OpakeAppview.Repo.Migrations.ExpandDirectoryAndDocumentTables do
  use Ecto.Migration

  def change do
    create table(:directories, primary_key: false) do
      add :directory_uri, :text, primary_key: true
      add :keyring_uri, :text
      add :owner_did, :text, null: false
      add :entries, {:array, :text}, null: false, default: []
      add :encrypted_metadata, :jsonb
      add :key_wrapping, :jsonb
      add :deleted_at, :utc_datetime_usec
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:directories, [:keyring_uri])
    create index(:directories, [:entries], using: :gin)
    create index(:directories, [:owner_did, :deleted_at])
    create index(:directories, [:deleted_at])

    create table(:documents, primary_key: false) do
      add :document_uri, :text, primary_key: true
      add :keyring_uri, :text
      add :owner_did, :text, null: false
      add :rotation, :integer
      add :encrypted_metadata, :jsonb
      add :encryption, :jsonb
      add :blob_ref, :jsonb
      add :deleted_at, :utc_datetime_usec
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:documents, [:keyring_uri])
    create index(:documents, [:owner_did, :deleted_at])
    create index(:documents, [:deleted_at])
  end
end
