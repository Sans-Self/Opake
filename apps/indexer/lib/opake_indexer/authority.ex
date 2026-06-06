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
        - the author was an editor AND the supersede is *additive*: every
          prior entry is either still present OR replaced by an added entry
          whose target record supersedes it (CIDs and ordering may differ).
      Viewers can't author directory supersedes.

      Additivity is **supersede-aware**: an editor may ADD their own entries
      and ADVANCE an existing entry by substituting a target that supersedes
      the one it replaces (a wiki-style edit). What an editor still cannot do
      is drop an entry outright — a removal with no superseding replacement
      is a disguised delete and is rejected. The supersede claim lives on the
      *added target record's* `supersedes` field, so the leaf doc must be
      indexed before the directory supersede that references it; the firehose
      consumer processes a commit's ops in write order, and curators emit the
      `applyWrites` bottom-up (substituting record first), so by the time the
      directory supersede is validated its replacement target is already in
      `records`.

    * **Workspace-root marker** — only managers may set `isWorkspaceRoot`,
      and the flag must not flip across a supersede. Enforced here so the
      dispatch can short-circuit before touching `chain_heads`.

  Roles are looked up against the current head keyring's `members` array
  via `RecordQueries.member_role/2` — pure JSONB read, no denormalized
  projection.

  The set of valid role strings is owned by this module (`@known_roles`)
  rather than by `RecordQueries`, because role semantics live with the
  authority rules. `RecordQueries` returns whatever string the JSONB
  contained; any value outside `@known_roles` is treated as
  `:insufficient_role` *and* logged as a `Logger.warning` so schema drift
  or data corruption surfaces in operations instead of silently rejecting
  the supersede.

  All checks return `:ok` or `{:rejected, reason :: atom()}`.
  """

  require Logger

  alias OpakeIndexer.Queries.RecordQueries

  @type result :: :ok | {:rejected, atom()}

  # Canonical role strings as they appear in `app.opake.keyring` records
  # (lexicon-defined; the Rust enum uses `serde(rename_all = "lowercase")`
  # → `Role::Manager → "manager"`, etc.). Any role outside this set is an
  # anomaly: lexicon drift, schema migration that didn't reach the indexer,
  # or tampered data. We log on the way through so the anomaly is
  # observable, then reject conservatively.
  @known_roles ~w(manager editor viewer)

  # -- Keyring authority --

  @doc """
  Validate a keyring supersede. The author must currently be a manager
  of the workspace. Genesis records (no prior head) skip the check.
  """
  @spec check_keyring_supersede(String.t(), String.t() | nil, String.t()) :: result()
  def check_keyring_supersede(_workspace_id, nil, _author_did), do: :ok

  def check_keyring_supersede(workspace_id, _prior_uri, author_did) do
    case classify_role(RecordQueries.member_role(workspace_id, author_did),
           workspace_id: workspace_id
         ) do
      :manager -> :ok
      :missing -> {:rejected, :not_a_member}
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
    case classify_role(RecordQueries.member_role(workspace_id, author_did),
           workspace_id: workspace_id
         ) do
      :manager ->
        :ok

      :editor ->
        additivity_check(prior_uri, new_entries)

      :missing ->
        {:rejected, :not_a_member}

      _ ->
        {:rejected, :insufficient_role}
    end
  end

  # -- Role classification --

  @doc """
  Normalize the raw role string from JSONB into a tagged atom.

  Known roles (`#{inspect(@known_roles)}`) map to their atom; `nil` means
  the DID isn't in the members list; anything else is anomalous data and
  gets logged via `Logger.warning` before being mapped to `:unknown` so
  callers' catch-all branches reject it.

  The `:workspace_id` option is included verbatim in the warning so the
  anomalous record is locatable from the log. Callers from `check_*`
  helpers pass the workspace they're validating; tests can pass any
  identifier (or omit it via `[]`).
  """
  @spec classify_role(String.t() | nil, keyword()) ::
          :manager | :editor | :viewer | :missing | :unknown
  def classify_role(role, opts \\ [])

  def classify_role(nil, _opts), do: :missing
  def classify_role("manager", _opts), do: :manager
  def classify_role("editor", _opts), do: :editor
  def classify_role("viewer", _opts), do: :viewer

  def classify_role(role, opts) when is_binary(role) do
    workspace_id = Keyword.get(opts, :workspace_id, "<unknown>")

    Logger.warning(
      "[Authority] unknown role #{inspect(role)} in workspace #{workspace_id} — " <>
        "expected one of #{inspect(@known_roles)}; rejecting as :insufficient_role"
    )

    :unknown
  end

  def classify_role(role, opts) do
    workspace_id = Keyword.get(opts, :workspace_id, "<unknown>")

    Logger.warning(
      "[Authority] non-string role #{inspect(role)} in workspace #{workspace_id} — " <>
        "rejecting as :insufficient_role"
    )

    :unknown
  end

  @doc """
  Pure additivity decision, given the resolved target sets.

    * `prior_targets` — target URIs in the prior canonical.
    * `new_targets` — target URIs in the superseding record.
    * `claimed_supersedes` — prior-target URIs that some *added* entry's
      target record claims to supersede (via its `supersedes` field). This
      is what lets an editor ADVANCE (edit) an entry rather than only ADD.

  Additive iff every dropped prior target (present before, absent now) is
  covered by a claimed supersede. A bare drop with no superseding entry is
  a disguised delete, so it is rejected. The DB-touching resolution lives
  in `additivity_check/2`; this part is pure so it is unit-testable.
  """
  @spec additive?(MapSet.t(), MapSet.t(), MapSet.t()) :: result()
  def additive?(prior_targets, new_targets, claimed_supersedes) do
    dropped = MapSet.difference(prior_targets, new_targets)

    if MapSet.subset?(dropped, claimed_supersedes) do
      :ok
    else
      {:rejected, :additivity_violation}
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
        dropped = MapSet.difference(prior_targets, new_targets)

        # Pure add / reorder: nothing dropped, so no supersede lookups needed.
        if MapSet.size(dropped) == 0 do
          :ok
        else
          added = MapSet.difference(new_targets, prior_targets)
          additive?(prior_targets, new_targets, claimed_supersedes(added))
        end

      _ ->
        # Prior record exists but has no entries field — treat as empty
        # (any supersede with any entries is additive).
        :ok
    end
  end

  # For each added entry, resolve its target record and read the
  # `supersedes` field: an added target that supersedes a prior entry is an
  # editor ADVANCE (edit), so the prior entry it replaces counts as covered.
  # Targets not yet indexed (or carrying no `supersedes`) contribute nothing
  # — an out-of-order arrival reads as a bare drop and is rejected, healing
  # on reprocess once the record lands, same posture as `:prior_not_indexed`.
  defp claimed_supersedes(added_targets) do
    added_targets
    |> Enum.map(&supersedes_of/1)
    |> Enum.reject(&is_nil/1)
    |> MapSet.new()
  end

  defp supersedes_of(target_uri) do
    case RecordQueries.lookup(target_uri) do
      %{record_jsonb: %{"supersedes" => s}} when is_binary(s) -> s
      _ -> nil
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
