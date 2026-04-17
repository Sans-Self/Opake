defmodule OpakeIndexer.Repo.Migrations.CreateDocumentUpdates do
  use Ecto.Migration

  def change do
    create table(:document_updates, primary_key: false) do
      add :uri, :text, primary_key: true
      add :document_uri, :text, null: false
      add :author_did, :text, null: false
      add :supersedes_uri, :text
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:document_updates, [:document_uri, :indexed_at, :uri])
    create index(:document_updates, [:author_did])
  end
end
