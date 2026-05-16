defmodule OpakeIndexer.Authority do
  @moduledoc """
  Authority validation for federation supersedes.

  Two authority models, one per chain type:

    * **Keyrings** — manager-only. A keyring supersede is valid iff the
      author is a manager of the workspace per the current head keyring's
      member list. For first-fault simplicity we use the current view;
      cross-chain time-travel verification is deferred.

    * **Directories** — manager-or-better OR editor-with-additivity. A
      directory supersede is valid iff:
        - the author was a manager at supersede time, OR
        - the author was an editor AND the new entries are a superset of
          the prior head's entries (set equality on target URIs; CIDs and
          ordering allowed to differ).
      Viewers can't author directory supersedes.

    * **Workspace-root marker** — only managers may set `isWorkspaceRoot`,
      and the flag must not flip across a supersede. Enforced here so the
      dispatch can short-circuit before touching `chain_heads`.

  Roles are looked up against the current head keyring's `members` array
  via `RecordQueries.member_role/2` — pure JSONB read, no denormalized
  projection.

  All checks return `:ok` or `{:rejected, reason :: atom()}`.
  """

  alias OpakeIndexer.Queries.RecordQueries

  @type result :: :ok | {:rejected, atom()}

  # -- Keyring authority --

  @doc """
  Validate a keyring supersede. The author must currently be a manager
  of the workspace. Genesis records (no prior head) skip the check.
  """
  @spec check_keyring_supersede(String.t(), String.t() | nil, String.t()) :: result()
  def check_keyring_supersede(_workspace_id, nil, _author_did), do: :ok

  def check_keyring_supersede(workspace_id, _prior_uri, author_did) do
    case RecordQueries.member_role(workspace_id, author_did) do
      "manager" -> :ok
      nil -> {:rejected, :not_a_member}
      _ -> {:rejected, :insufficient_role}
    end
  end

  # -- Directory authority --

  @doc """
  Validate a directory supersede. Genesis directories skip the check;
  supersedes require manager OR editor-with-additivity.

  `new_entries` is the list parsed straight from `record_jsonb["entries"]`
  — each entry has `"target"` and `"targetCid": {"$link": cid}`. We only
  compare on `"target"`; CIDs and ordering may differ across an editor's
  additive supersede.
  """
  @spec check_directory_supersede(
          String.t(),
          String.t() | nil,
          String.t(),
          [map()]
        ) :: result()
  def check_directory_supersede(_workspace_id, nil, _author_did, _new_entries), do: :ok

  def check_directory_supersede(workspace_id, prior_uri, author_did, new_entries) do
    case RecordQueries.member_role(workspace_id, author_did) do
      "manager" ->
        :ok

      "editor" ->
        additivity_check(prior_uri, new_entries)

      nil ->
        {:rejected, :not_a_member}

      _ ->
        {:rejected, :insufficient_role}
    end
  end

  defp additivity_check(prior_uri, new_entries) do
    case RecordQueries.lookup(prior_uri) do
      nil ->
        # Without the prior we can't verify additivity. Reject
        # conservatively; the live state heals once the prior arrives.
        {:rejected, :prior_not_indexed}

      %{record_jsonb: %{"entries" => prior_entries}} when is_list(prior_entries) ->
        prior_targets = MapSet.new(prior_entries, &entry_target/1)
        new_targets = MapSet.new(new_entries, &entry_target/1)

        if MapSet.subset?(prior_targets, new_targets) do
          :ok
        else
          {:rejected, :additivity_violation}
        end

      _ ->
        # Prior record exists but has no entries field — treat as empty
        # (any supersede with any entries is additive).
        :ok
    end
  end

  # -- Workspace-root marker --

  @doc """
  Validate the `isWorkspaceRoot` flag's transition across a directory
  supersede. The flag must not flip; flipping false→true requires manager
  authority and must be paired with a fresh root-chain genesis (no prior).
  Flipping true→false is always invalid.

  Caller passes the prior record (if any) and the new record's flag.
  """
  @spec check_workspace_root_flag(map() | nil, boolean()) :: result()
  def check_workspace_root_flag(nil, _new_flag), do: :ok

  def check_workspace_root_flag(%{record_jsonb: %{"isWorkspaceRoot" => prior_flag}}, new_flag)
      when prior_flag === new_flag,
      do: :ok

  def check_workspace_root_flag(%{record_jsonb: prior_jsonb}, false)
      when not is_map_key(prior_jsonb, "isWorkspaceRoot"),
      do: :ok

  def check_workspace_root_flag(_prior, _new_flag) do
    {:rejected, :workspace_root_flip}
  end

  defp entry_target(%{"target" => t}), do: t
  defp entry_target(%{target: t}), do: t
  defp entry_target(_), do: nil
end
