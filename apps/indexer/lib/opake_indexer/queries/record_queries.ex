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

  @directory_collection "app.opake.directory"
  @document_collection "app.opake.document"
  @keyring_collection "app.opake.keyring"
  @grant_collection "app.opake.grant"

  @spec upsert(map()) :: {:ok, RecordSchema.t()} | {:error, Ecto.Changeset.t()}
  def upsert(attrs) do
    %RecordSchema{}
    |> RecordSchema.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace_all_except, [:uri]},
      conflict_target: :uri
    )
  end

  @spec soft_delete(String.t(), DateTime.t()) :: {non_neg_integer(), nil}
  def soft_delete(uri, now) do
    from(r in RecordSchema, where: r.uri == ^uri)
    |> Repo.update_all(set: [deleted_at: now])
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
          (r.indexed_at > ^since or r.deleted_at > ^since)
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
          (r.indexed_at > ^since or r.deleted_at > ^since)
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

  @doc """
  True iff `did` is a member of `workspace_id`.
  """
  @spec is_member?(String.t(), String.t()) :: boolean()
  def is_member?(workspace_id, did) do
    not is_nil(member_role(workspace_id, did))
  end

  @doc """
  The `did => role` map of the current head keyring's members, or `nil`
  if no keyring has been indexed for the workspace.
  """
  @spec head_member_roles(String.t()) :: %{String.t() => String.t()} | nil
  def head_member_roles(workspace_id) do
    case workspace_keyring_head(workspace_id) do
      nil ->
        nil

      %RecordSchema{record_jsonb: %{"members" => members}} when is_list(members) ->
        Map.new(members, fn
          %{"wrappedKey" => %{"did" => did}, "role" => role} -> {did, role}
          _ -> {nil, nil}
        end)

      %RecordSchema{} ->
        %{}
    end
  end

  defp extract_member_role(%{"members" => members}, did) when is_list(members) do
    Enum.find_value(members, fn
      %{"wrappedKey" => %{"did" => ^did}, "role" => role} -> role
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

    containment = %{"members" => [%{"wrappedKey" => %{"did" => did}}]}

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
