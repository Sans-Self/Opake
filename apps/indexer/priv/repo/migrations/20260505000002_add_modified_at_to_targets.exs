defmodule OpakeIndexer.Repo.Migrations.AddModifiedAtToTargets do
  @moduledoc """
  Adds `modified_at` to documents, directories, and keyrings so the editor's
  proposal-cleanup heuristic (delete proposal when target's modifiedAt advances
  past proposal's createdAt) can run via the indexer's bootstrap responses
  without per-target XRPC round-trips.
  """

  use Ecto.Migration

  def change do
    alter table(:documents) do
      add :modified_at, :string
    end

    alter table(:directories) do
      add :modified_at, :string
    end

    alter table(:keyrings) do
      add :modified_at, :string
    end
  end
end
