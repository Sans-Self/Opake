defmodule OpakeIndexer.Repo.Migrations.CreateWorkspaceDirectories do
  use Ecto.Migration

  def change do
    create table(:workspace_directories, primary_key: false) do
      add :directory_uri, :text, primary_key: true
      add :keyring_uri, :text, null: false
      add :owner_did, :text, null: false
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:workspace_directories, [:keyring_uri, :indexed_at, :directory_uri])
  end
end
