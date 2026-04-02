defmodule OpakeAppview.Repo.Migrations.CreateKeyringUpdates do
  use Ecto.Migration

  def change do
    create table(:keyring_updates, primary_key: false) do
      add :uri, :string, primary_key: true
      add :keyring_uri, :string, null: false
      add :author_did, :string, null: false
      add :action_type, :string, null: false
      add :member_did, :string
      add :member_public_key, :binary
      add :role, :string
      add :encrypted_metadata, :map
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:keyring_updates, [:keyring_uri])
  end
end
