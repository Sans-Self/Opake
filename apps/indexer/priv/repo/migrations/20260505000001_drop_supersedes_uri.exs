defmodule OpakeIndexer.Repo.Migrations.DropSupersedesUri do
  use Ecto.Migration

  def change do
    alter table(:document_updates) do
      remove :supersedes_uri, :text
    end
  end
end
