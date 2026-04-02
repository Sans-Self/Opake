defmodule OpakeAppview.Repo.Migrations.EnrichDirectoryUpdates do
  use Ecto.Migration

  def change do
    alter table(:directory_updates) do
      add :encrypted_metadata, :map
      add :source_directory_uri, :text
      add :target_directory_uri, :text
      add :parent_directory_uri, :text
    end
  end
end
