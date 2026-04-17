defmodule OpakeIndexer.Repo.Migrations.AddCompositePaginationIndexes do
  use Ecto.Migration

  def change do
    # Covers list_inbox filter + sort: WHERE recipient_did = ? ORDER BY indexed_at DESC, uri DESC
    # Subsumes the single-column recipient_did index
    drop index(:grants, [:recipient_did])
    create index(:grants, [:recipient_did, :indexed_at, :uri])

    # Covers list_keyrings_for_member: WHERE member_did = ? ORDER BY indexed_at DESC, keyring_uri DESC
    # Subsumes the single-column member_did index
    drop index(:keyring_members, [:member_did])
    create index(:keyring_members, [:member_did, :indexed_at, :keyring_uri])
  end
end
