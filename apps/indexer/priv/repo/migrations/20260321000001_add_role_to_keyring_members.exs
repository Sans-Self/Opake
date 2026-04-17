defmodule OpakeIndexer.Repo.Migrations.AddRoleToKeyringMembers do
  use Ecto.Migration

  def change do
    alter table(:keyring_members) do
      add :role, :text
    end
  end
end
