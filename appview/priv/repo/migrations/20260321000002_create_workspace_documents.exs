defmodule OpakeAppview.Repo.Migrations.CreateWorkspaceDocuments do
  use Ecto.Migration

  def change do
    create table(:workspace_documents, primary_key: false) do
      add :document_uri, :text, primary_key: true
      add :keyring_uri, :text, null: false
      add :owner_did, :text, null: false
      add :rotation, :integer, null: false, default: 0
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:workspace_documents, [:keyring_uri, :indexed_at, :document_uri])
    create index(:workspace_documents, [:owner_did])
  end
end
