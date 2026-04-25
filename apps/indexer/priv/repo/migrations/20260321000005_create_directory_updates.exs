defmodule OpakeIndexer.Repo.Migrations.CreateDirectoryUpdates do
  use Ecto.Migration

  def change do
    create table(:directory_updates, primary_key: false) do
      add :uri, :text, primary_key: true
      add :keyring_uri, :text, null: false
      add :author_did, :text, null: false
      add :action_type, :text, null: false
      add :directory_uri, :text
      add :entry_uri, :text
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:directory_updates, [:keyring_uri, :indexed_at, :uri])
  end
end
