defmodule OpakeIndexer.Queries.RecordQueries do
  @moduledoc """
  CRUD + list queries over `records`. Records are upserted on firehose
  create/update events and soft-deleted on delete events.

  Collection-scoped helpers are provided as thin wrappers — the underlying
  table is one row per record regardless of collection, so most queries
  filter by `collection` first.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Record, as: RecordSchema

  @directory_collection "at.opake.directory"
  @document_collection "at.opake.document"
  @keyring_collection "at.opake.keyring"
  @grant_collection "at.opake.grant"

  @doc """
  Upsert a record. `updated_at` is the last-write watermark: it is stamped
  fresh from the event's `indexed_at` (the write's wall-clock now) on both the
  insert and the conflict-update path. On conflict `indexed_at` is preserved
  (first-seen, immutable) while `updated_at` is replaced — it is not in the
  `replace_all_except` list — so `changes_since` re-delivers in-place updates
  without repositioning the record's pagination order.

  See spec:indexer-consistency § indexed_at is first-seen.
  """
  @spec upsert(map()) :: {:ok, RecordSchema.t()} | {:error, Ecto.Changeset.t()}
  def upsert(attrs) do
    %RecordSchema{}
    |> RecordSchema.changeset(stamp_updated_at(attrs))
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri, :indexed_at]},
      conflict_target: :uri
    )
  end

  # `updated_at` tracks the write's wall-clock now, which the dispatch path
  # carries in `indexed_at`. Deriving it here keeps every caller (firehose
  # dispatch and tests) honest without threading a second timestamp through.
  defp stamp_updated_at(attrs) do
    now = Map.get(attrs, :indexed_at) || Map.get(attrs, "indexed_at")
    Map.put(attrs, :updated_at, now)
  end

  @spec soft_delete(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete(uri, now) do
    from(r in RecordSchema, where: r.uri == ^uri)
    |> Repo.update_all(set: [deleted_at: now, updated_at: now])
  end

  @spec lookup(String.t()) :: RecordSchema.t() | nil
  def lookup(uri), do: Repo.get(RecordSchema, uri)

  # -- Workspace tree fetches ----------------------------------------

  @spec workspace_tree(String.t()) :: {[RecordSchema.t()], [RecordSchema.t()]}
  def workspace_tree(workspace_id) do
    {workspace_records(workspace_id, @directory_collection),
     workspace_records(workspace_id, @document_collection)}
  end

  @spec workspace_changes_since(String.t(), DateTime.t()) ::
          {[RecordSchema.t()], [RecordSchema.t()]}
  def workspace_changes_since(workspace_id, since) do
    {workspace_changes(workspace_id, @directory_collection, since),
     workspace_changes(workspace_id, @document_collection, since)}
  end

  @spec cabinet_tree(String.t()) :: {[RecordSchema.t()], [RecordSchema.t()]}
  def cabinet_tree(author_did) do
    {cabinet_records(author_did, @directory_collection),
     cabinet_records(author_did, @document_collection)}
  end

  @spec cabinet_changes_since(String.t(), DateTime.t()) ::
          {[RecordSchema.t()], [RecordSchema.t()]}
  def cabinet_changes_since(author_did, since) do
    {cabinet_changes(author_did, @directory_collection, since),
     cabinet_changes(author_did, @document_collection, since)}
  end

  defp workspace_records(workspace_id, collection) do
    from(r in RecordSchema,
      where:
        r.collection == ^collection and
          r.workspace_id == ^workspace_id and
          is_nil(r.deleted_at)
    )
    |> Repo.all()
  end

  defp workspace_changes(workspace_id, collection, since) do
    from(r in RecordSchema,
      where:
        r.collection == ^collection and
          r.workspace_id == ^workspace_id and
          (r.updated_at > ^since or r.deleted_at > ^since)
    )
    |> Repo.all()
  end

  defp cabinet_records(author_did, collection) do
    from(r in RecordSchema,
      where:
        r.collection == ^collection and
          r.author_did == ^author_did and
          is_nil(r.workspace_id) and
          is_nil(r.deleted_at)
    )
    |> Repo.all()
  end

  defp cabinet_changes(author_did, collection, since) do
    from(r in RecordSchema,
      where:
        r.collection == ^collection and
          r.author_did == ^author_did and
          is_nil(r.workspace_id) and
          (r.updated_at > ^since or r.deleted_at > ^since)
    )
    |> Repo.all()
  end

  # -- Keyring head fetch --------------------------------------------

  @doc """
  Fetch the keyring chain-head record for a workspace, returning the full
  record row (so callers get the verbatim member list inside `record_jsonb`).
  Returns `nil` if no keyring has been indexed for the workspace yet.
  """
  @spec workspace_keyring_head(String.t()) :: RecordSchema.t() | nil
  def workspace_keyring_head(workspace_id) do
    alias OpakeIndexer.Schemas.ChainHead

    Repo.one(
      from(c in ChainHead,
        join: r in RecordSchema,
        on: r.uri == c.head_uri,
        where: c.workspace_id == ^workspace_id and c.kind == "keyring",
        select: r
      )
    )
  end

  @doc """
  The newest live keyring record for a workspace — the rollback target
  when the chain head is deleted. Newest-live wins over the tombstone's
  `supersedes` link, which can dangle once intermediate tombstones are
  purged (see `TombstoneCleanup`). Returns `nil` when no live record
  remains, i.e. the chain is torn down.
  """
  @spec newest_live_keyring(String.t()) :: RecordSchema.t() | nil
  def newest_live_keyring(workspace_id) do
    from(r in RecordSchema,
      where:
        r.collection == ^@keyring_collection and
          r.workspace_id == ^workspace_id and
          is_nil(r.deleted_at),
      order_by: [desc: r.indexed_at, desc: r.uri],
      limit: 1
    )
    |> Repo.one()
  end

  # -- Membership query ----------------------------------------------

  @doc """
  Returns the role of `did` inside the current head keyring of `workspace_id`,
  or `nil` if `did` is not a member. Reads from the live keyring record's
  JSONB members list — no denormalized projection.
  """
  @spec member_role(String.t(), String.t()) :: String.t() | nil
  def member_role(workspace_id, did) do
    case workspace_keyring_head(workspace_id) do
      nil ->
        nil

      %RecordSchema{record_jsonb: jsonb} ->
        extract_member_role(jsonb, did)
    end
  end

  @typedoc """
  Membership resolution for a (workspace, did) pair. `:workspace_not_indexed`
  means no keyring chain head exists — the genesis has not been consumed yet,
  or the chain was torn down; the two are not distinguishable and the indexer
  has nothing to answer for either way. `:not_a_member` means a head was
  consulted and the DID is absent from its member list, which makes it a
  definitive authorization answer rather than a lag artifact.

  See spec:indexer-consistency § Unknown workspace is distinguishable from
  non-membership.
  """
  @type membership :: :workspace_not_indexed | :not_a_member | {:member, String.t()}

  @doc """
  Resolve `did`'s membership in `workspace_id` against the head keyring.
  """
  @spec resolve_membership(String.t(), String.t()) :: membership()
  def resolve_membership(workspace_id, did) do
    case workspace_keyring_head(workspace_id) do
      nil ->
        :workspace_not_indexed

      %RecordSchema{record_jsonb: jsonb} ->
        case extract_member_role(jsonb, did) do
          nil -> :not_a_member
          role -> {:member, role}
        end
    end
  end

  defp extract_member_role(%{"members" => members}, did) when is_list(members) do
    Enum.find_value(members, fn
      %{"did" => ^did, "role" => role} -> role
      _ -> nil
    end)
  end

  defp extract_member_role(_, _), do: nil

  # -- Workspace listing for a member --------------------------------

  @doc """
  Returns the head keyring records for every workspace `did` is currently a
  member of. Uses JSONB containment so the GIN index can serve it.
  """
  @spec workspaces_for_member(String.t()) :: [RecordSchema.t()]
  def workspaces_for_member(did) do
    alias OpakeIndexer.Schemas.ChainHead

    containment = %{"members" => [%{"did" => did}]}

    Repo.all(
      from(c in ChainHead,
        join: r in RecordSchema,
        on: r.uri == c.head_uri,
        where:
          c.kind == "keyring" and
            fragment("? @> ?", r.record_jsonb, ^containment),
        select: r
      )
    )
  end

  # -- Inbox (grants for a recipient) --------------------------------

  @doc """
  Returns grant records where `record_jsonb->>'recipient' == did`, newest
  first. Cursor pagination on `(indexed_at, uri)`.
  """
  @spec inbox(String.t(), keyword()) :: {[RecordSchema.t()], String.t() | nil}
  def inbox(did, opts \\ []) do
    alias OpakeIndexer.Queries.Pagination

    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(r in RecordSchema,
        where:
          r.collection == ^@grant_collection and
            is_nil(r.deleted_at) and
            fragment("?->>'recipient' = ?", r.record_jsonb, ^did),
        order_by: [desc: r.indexed_at, desc: r.uri],
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, datetime, uri} ->
          from(r in query,
            where:
              r.indexed_at < ^datetime or
                (r.indexed_at == ^datetime and r.uri < ^uri)
          )

        :none ->
          query
      end

    results = Repo.all(query)
    {results, Pagination.build_next_cursor(results)}
  end

  # -- Constants exposed for callers ---------------------------------

  def directory_collection, do: @directory_collection
  def document_collection, do: @document_collection
  def keyring_collection, do: @keyring_collection
  def grant_collection, do: @grant_collection
end
