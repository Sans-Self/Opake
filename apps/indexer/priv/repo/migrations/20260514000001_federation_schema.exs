defmodule OpakeIndexer.Repo.Migrations.FederationSchema do
  @moduledoc """
  Lean federation schema. Two record-bearing tables (records + chain_heads)
  plus the firehose cursor. atproto record JSON is stored verbatim as JSONB;
  structural columns are pure projections of the JSONB, extracted only for
  index speed.

  Tables:

    * `cursor` — singleton firehose cursor row.
    * `records` — every indexed `app.opake.*` record, one row per (uri).
    * `chain_heads` — current head URI per workspace chain (keyring chain
      and workspace-root chain).

  Drop-and-recreate from empty — branch has never shipped; no backfill.
  """

  use Ecto.Migration

  def change do
    # -- Cursor ---------------------------------------------------------

    create table(:cursor, primary_key: false) do
      add :id, :integer, primary_key: true
      add :time_us, :bigint, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    create constraint(:cursor, :cursor_singleton, check: "id = 1")

    # -- Records --------------------------------------------------------
    #
    # `record_jsonb` is the verbatim on-PDS record JSON (camelCase, no
    # field reshaping). Structural columns are projections — invariants
    # enforced by the firehose dispatch, not by triggers (cheaper, and
    # the dispatch is the only writer).

    create table(:records, primary_key: false) do
      add :uri, :text, primary_key: true
      add :collection, :text, null: false
      add :author_did, :text, null: false
      add :workspace_id, :text
      add :supersedes_uri, :text
      add :is_workspace_root, :boolean, null: false, default: false
      add :cid, :text, null: false
      add :indexed_at, :utc_datetime_usec, null: false
      add :deleted_at, :utc_datetime_usec
      add :record_jsonb, :map, null: false
    end

    create index(:records, [:collection, :workspace_id, :indexed_at])
    create index(:records, [:collection, :author_did, :indexed_at],
             where: "workspace_id IS NULL",
             name: :records_cabinet_idx
           )
    create index(:records, [:supersedes_uri])
    create index(:records, [:workspace_id, :is_workspace_root, :indexed_at],
             where: "is_workspace_root",
             name: :records_workspace_root_idx
           )

    # GIN containment index supporting membership / recipient queries:
    #   - `record_jsonb @> '{"members":[{"wrappedKey":{"did":...}}]}'`
    #     locates keyrings a user is a member of.
    #   - `record_jsonb @> '{"recipient":"did:plc:..."}'` locates grants
    #     by recipient. Cheap because jsonb_path_ops is small.
    execute(
      "CREATE INDEX records_jsonb_gin ON records USING gin (record_jsonb jsonb_path_ops)",
      "DROP INDEX IF EXISTS records_jsonb_gin"
    )

    # -- Chain heads ----------------------------------------------------
    #
    # One row per active chain. `kind = 'keyring'` for the workspace
    # keyring chain (one per workspace). `kind = 'workspace_root'` for
    # the workspace-root directory chain (one per workspace).
    #
    # Subdirectory chains are not tracked here — clients resolve them
    # via `supersedes_uri` back-edges from the workspace root. When/if
    # benchmarks demand per-path indexing, add a `'directory'` kind with
    # a `path` column and a per-workspace uniqueness constraint.

    create table(:chain_heads, primary_key: false) do
      add :workspace_id, :text, primary_key: true
      add :kind, :text, primary_key: true
      add :head_uri, :text, null: false
      add :head_cid, :text, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    create constraint(:chain_heads, :chain_heads_kind_check,
             check: "kind IN ('keyring', 'workspace_root')"
           )
  end
end
