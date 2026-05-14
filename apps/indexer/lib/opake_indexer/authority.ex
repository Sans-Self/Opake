defmodule OpakeIndexer.Authority do
  @moduledoc """
  Authority validation for federation supersedes.

  Two authority models, one per chain type:

    * **Keyrings** — manager-only. A keyring supersede is valid iff the
      record's author was a manager of the *prior* head's membership at
      the time of the supersede. Membership is checked against the
      indexer's materialized view (which reflects the head as of "now",
      not the historical head). For first-fault simplicity we use the
      current view; cross-chain time-travel verification is deferred
      until we have a reason to care.

    * **Directories** — manager-or-better OR editor-with-additivity. A
      directory supersede is valid iff:
        - the author was a manager at supersede time, OR
        - the author was an editor AND the new `entries_json` is a
          superset of the prior head's `entries_json` (set equality on
          target URIs; CIDs and ordering are allowed to differ).
      Viewers can't author directory supersedes.

  Roles are looked up via `KeyringQueries.member_role/2`. Unknown role
  values are treated as no-authority.

  All checks return `:ok` or `{:rejected, reason :: atom()}`. Callers in
  `OpakeIndexer.Firehose` log the rejection at warning level and refuse
  to advance the chain.
  """

  alias OpakeIndexer.Queries.{DirectoryQueries, KeyringQueries}

  @type result :: :ok | {:rejected, atom()}

  # -- Keyring authority --

  @doc """
  Validate a keyring supersede. The author must have been a manager of
  the workspace before the supersede landed.

  Genesis records (no prior head) skip authority checks — anyone can
  write a workspace into existence on their own PDS. The genesis URI
  becomes the workspace identity.
  """
  @spec check_keyring_supersede(String.t(), String.t() | nil, String.t()) :: result()
  def check_keyring_supersede(_workspace_id, nil, _author_did), do: :ok

  def check_keyring_supersede(workspace_id, _prior_uri, author_did) do
    case KeyringQueries.member_role(workspace_id, author_did) do
      "manager" -> :ok
      nil -> {:rejected, :not_a_member}
      _ -> {:rejected, :insufficient_role}
    end
  end

  # -- Directory authority --

  @doc """
  Validate a directory supersede. Genesis directories skip the check;
  supersedes require manager OR editor-with-additivity.

  `new_entries` is `[%{"target" => uri, "target_cid" => cid}, ...]` from
  the new directory record's `entries_json`. The prior head's entries
  are loaded from the indexer's directory store.
  """
  @spec check_directory_supersede(
          String.t(),
          String.t() | nil,
          String.t(),
          [map()]
        ) :: result()
  def check_directory_supersede(_workspace_id, nil, _author_did, _new_entries), do: :ok

  def check_directory_supersede(workspace_id, prior_uri, author_did, new_entries) do
    case KeyringQueries.member_role(workspace_id, author_did) do
      "manager" ->
        :ok

      "editor" ->
        additivity_check(prior_uri, new_entries)

      nil ->
        {:rejected, :not_a_member}

      _ ->
        # viewer or unknown
        {:rejected, :insufficient_role}
    end
  end

  defp additivity_check(prior_uri, new_entries) do
    case DirectoryQueries.lookup(prior_uri) do
      nil ->
        # Prior head not indexed — without it we can't verify additivity.
        # Reject conservatively; the live state will heal once the prior
        # arrives and the editor retries.
        {:rejected, :prior_not_indexed}

      prior ->
        prior_targets = MapSet.new(prior.entries_json, &entry_target/1)
        new_targets = MapSet.new(new_entries, &entry_target/1)

        cond do
          MapSet.subset?(prior_targets, new_targets) ->
            :ok

          true ->
            {:rejected, :additivity_violation}
        end
    end
  end

  defp entry_target(%{"target" => t}), do: t
  defp entry_target(%{target: t}), do: t
  defp entry_target(_), do: nil
end
