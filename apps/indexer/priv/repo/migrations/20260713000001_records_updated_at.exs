defmodule OpakeIndexer.Repo.Migrations.RecordsUpdatedAt do
  @moduledoc """
  Add a last-write watermark to `records`.

  `indexed_at` is first-seen and immutable (pagination order keys to it), so a
  delta filter built on it goes blind to in-place record updates — directory
  records mutate in place under curatorial writes, and incremental sync would
  silently lose every such mutation. `updated_at` is set on every upsert and is
  the column `changes_since` filters on.

  Backfilled from `indexed_at` so pre-existing rows carry a coherent watermark;
  NOT NULL thereafter. Delta queries filter on it under the same
  (collection, workspace_id) / (collection, author_did) shapes that index
  `indexed_at`, so the watermark gets mirror indexes.

  See spec:indexer-consistency § indexed_at is first-seen.
  """

  use Ecto.Migration

  def up do
    alter table(:records) do
      add :updated_at, :utc_datetime_usec
    end

    execute("UPDATE records SET updated_at = indexed_at")

    alter table(:records) do
      modify :updated_at, :utc_datetime_usec, null: false
    end

    create index(:records, [:collection, :workspace_id, :updated_at])

    create index(:records, [:collection, :author_did, :updated_at],
             where: "workspace_id IS NULL",
             name: :records_cabinet_updated_idx
           )
  end

  def down do
    drop index(:records, [:collection, :workspace_id, :updated_at])
    drop index(:records, [:collection, :author_did, :updated_at], name: :records_cabinet_updated_idx)

    alter table(:records) do
      remove :updated_at
    end
  end
end
