defmodule OpakeAppview.Repo.Migrations.CreateKeyringMembers do
  use Ecto.Migration

  def change do
    create table(:keyring_members, primary_key: false) do
      add :keyring_uri, :text, null: false
      add :member_did, :text, null: false
      add :owner_did, :text, null: false
      add :indexed_at, :utc_datetime_usec, null: false
    end

    execute(
      "ALTER TABLE keyring_members ADD PRIMARY KEY (keyring_uri, member_did)",
      ""
    )

    create index(:keyring_members, [:member_did])
  end
end
