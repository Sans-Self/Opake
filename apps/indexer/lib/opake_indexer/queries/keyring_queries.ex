defmodule OpakeIndexer.Queries.KeyringQueries do
  @moduledoc """
  Keyring CRUD and membership queries. Keyrings are immutable chain members;
  current head pointers live in `OpakeIndexer.Queries.KeyringChainQueries`.

  `keyring_members` reflects the *current* head's members — replaced
  wholesale on every supersede via `replace_members/2` (delete-all-then-
  insert in a transaction).

  Workspace identity is `workspace_id` (= genesis keyring URI). All
  member-keyed queries take `workspace_id`, not the keyring URI.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.{Keyring, KeyringMember}
  alias OpakeIndexer.Queries.Pagination

  @spec upsert_keyring_record(map()) :: {:ok, Keyring.t()} | {:error, Ecto.Changeset.t()}
  def upsert_keyring_record(attrs) do
    %Keyring{}
    |> Keyring.changeset(%{
      uri: attrs.uri,
      workspace_id: attrs[:workspace_id],
      rotation: attrs[:rotation] || 0,
      encrypted_metadata: attrs[:encrypted_metadata],
      supersedes_uri: attrs[:supersedes_uri],
      created_at: attrs[:created_at],
      modified_at: attrs[:modified_at],
      indexed_at: DateTime.utc_now()
    })
    |> Repo.insert(
      on_conflict:
        {:replace,
         [
           :workspace_id,
           :rotation,
           :encrypted_metadata,
           :supersedes_uri,
           :created_at,
           :modified_at,
           :indexed_at
         ]},
      conflict_target: :uri
    )
  end

  @spec lookup(String.t()) :: Keyring.t() | nil
  def lookup(uri), do: Repo.get(Keyring, uri)

  @doc """
  Replace the membership rows for a workspace with the head keyring's
  member list. Delete-all-then-insert in a transaction so the visible
  state matches the keyring's "full member list" semantics — partial
  changes are never observable.
  """
  @spec replace_members(String.t(), [map()]) :: {:ok, term()} | {:error, term()}
  def replace_members(workspace_id, member_entries) do
    now = DateTime.utc_now()

    Repo.transaction(fn ->
      from(km in KeyringMember, where: km.workspace_id == ^workspace_id)
      |> Repo.delete_all()

      rows =
        Enum.map(member_entries, fn entry ->
          %{
            workspace_id: workspace_id,
            member_did: entry.did,
            role: entry[:role],
            wrapped_key: entry[:wrapped_key],
            indexed_at: now
          }
        end)

      Repo.insert_all(KeyringMember, rows)
    end)
  end

  @spec delete_workspace(String.t()) :: :ok
  def delete_workspace(workspace_id) do
    from(km in KeyringMember, where: km.workspace_id == ^workspace_id)
    |> Repo.delete_all()

    from(k in Keyring, where: k.workspace_id == ^workspace_id)
    |> Repo.delete_all()

    :ok
  end

  @spec list_workspaces_for_member(String.t(), keyword()) :: {[map()], String.t() | nil}
  def list_workspaces_for_member(member_did, opts \\ []) do
    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    query =
      from(km in KeyringMember,
        where: km.member_did == ^member_did,
        order_by: [desc: km.indexed_at, desc: km.workspace_id],
        select: %{
          workspace_id: km.workspace_id,
          indexed_at: km.indexed_at
        },
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from(km in query,
            where:
              km.indexed_at < ^cursor_time or
                (km.indexed_at == ^cursor_time and km.workspace_id < ^cursor_uri)
          )

        :none ->
          query
      end

    workspaces = Repo.all(query)
    next_cursor = build_workspace_cursor(workspaces)

    {workspaces, next_cursor}
  end

  @doc """
  List workspaces a DID is a member of, joined with the current head keyring
  data. Returns `{[{Keyring.t(), [KeyringMember.t()]}], cursor}` where the
  keyring is the chain head (not all chain members).
  """
  @spec list_workspaces_full(String.t(), keyword()) ::
          {[{Keyring.t(), [KeyringMember.t()]}], String.t() | nil}
  def list_workspaces_full(member_did, opts \\ []) do
    alias OpakeIndexer.Schemas.KeyringChain

    limit = Keyword.get(opts, :limit, 50)
    cursor = Keyword.get(opts, :cursor)

    # Find workspaces the DID is a member of; join through the chain
    # head to fetch the current keyring record.
    base_query =
      from(km in KeyringMember,
        join: kc in KeyringChain,
        on: kc.workspace_id == km.workspace_id,
        join: k in Keyring,
        on: k.uri == kc.head_uri,
        where: km.member_did == ^member_did,
        order_by: [desc: k.indexed_at, desc: k.uri],
        select: k,
        limit: ^limit
      )

    query =
      case Pagination.parse_cursor(cursor) do
        {:ok, cursor_time, cursor_uri} ->
          from([km, kc, k] in base_query,
            where:
              k.indexed_at < ^cursor_time or
                (k.indexed_at == ^cursor_time and k.uri < ^cursor_uri)
          )

        :none ->
          base_query
      end

    keyrings = Repo.all(query)
    next_cursor = Pagination.build_next_cursor(keyrings)

    workspace_ids = Enum.map(keyrings, & &1.workspace_id)

    members_by_workspace =
      if workspace_ids == [] do
        %{}
      else
        from(km in KeyringMember, where: km.workspace_id in ^workspace_ids)
        |> Repo.all()
        |> Enum.group_by(& &1.workspace_id)
      end

    pairs =
      Enum.map(keyrings, fn k ->
        {k, Map.get(members_by_workspace, k.workspace_id, [])}
      end)

    {pairs, next_cursor}
  end

  @spec is_member?(String.t(), String.t()) :: boolean()
  def is_member?(workspace_id, did) do
    from(km in KeyringMember,
      where: km.workspace_id == ^workspace_id and km.member_did == ^did
    )
    |> Repo.exists?()
  end

  @doc """
  Return the role of a DID in a workspace, or `nil` if they're not a member.
  Used by `OpakeIndexer.Authority` to validate manager-only supersedes.
  """
  @spec member_role(String.t(), String.t()) :: String.t() | nil
  def member_role(workspace_id, did) do
    from(km in KeyringMember,
      where: km.workspace_id == ^workspace_id and km.member_did == ^did,
      select: km.role
    )
    |> Repo.one()
  end

  @spec all_member_dids() :: [String.t()]
  def all_member_dids do
    from(km in KeyringMember, select: km.member_did, distinct: true)
    |> Repo.all()
  end

  @spec workspace_count() :: non_neg_integer()
  def workspace_count do
    from(km in KeyringMember, select: count(km.workspace_id, :distinct))
    |> Repo.one()
  end

  # -- Internal --

  defp build_workspace_cursor([]), do: nil

  defp build_workspace_cursor(rows) do
    last = List.last(rows)

    case last do
      %{indexed_at: ts, workspace_id: id} ->
        "#{DateTime.to_iso8601(ts)}::#{id}"

      _ ->
        nil
    end
  end
end
