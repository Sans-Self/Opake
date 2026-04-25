defmodule OpakeIndexer.Repo.Migrations.CreateKeyrings do
  use Ecto.Migration

  def change do
    create table(:keyrings, primary_key: false) do
      add :uri, :text, primary_key: true
      add :owner_did, :text, null: false
      add :rotation, :bigint, null: false, default: 0
      add :members, :jsonb, null: false, default: "[]"
      add :encrypted_metadata, :jsonb
      add :created_at, :text
      add :indexed_at, :utc_datetime_usec, null: false
    end

    create index(:keyrings, [:owner_did])
  end
end
