defmodule OpakeAppview.Repo.Migrations.CreateGrants do
  use Ecto.Migration

  def change do
    create table(:grants, primary_key: false) do
      add :uri, :text, primary_key: true
      add :owner_did, :text, null: false
      add :recipient_did, :text, null: false
      add :document_uri, :text, null: false
      add :created_at, :text, null: false
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:grants, [:recipient_did])
    create index(:grants, [:owner_did])
  end
end
