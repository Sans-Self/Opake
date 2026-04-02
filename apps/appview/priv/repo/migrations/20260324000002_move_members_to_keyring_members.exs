defmodule OpakeAppview.Repo.Migrations.MoveMembersToKeyringMembers do
  use Ecto.Migration

  def change do
    # Add wrapped_key to keyring_members so the full member payload
    # lives in one place (no duplication with keyrings.members).
    alter table(:keyring_members) do
      add :wrapped_key, :map
    end

    alter table(:keyrings) do
      remove :members, {:array, :map}, default: []
    end
  end
end
