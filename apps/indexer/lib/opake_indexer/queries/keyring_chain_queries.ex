defmodule OpakeIndexer.Queries.KeyringChainQueries do
  @moduledoc """
  Bookkeeping for keyring supersede chain heads. One row per workspace.

  `advance/4` is a compare-and-set: it only updates the head when the
  caller's `expected_prior_uri` matches the stored `head_uri`. This is
  how the indexer detects forks — if two clients both supersede the same
  prior head concurrently, the second one's `advance` returns
  `:fork_detected` and the firehose dispatches a `chain-forked` event
  instead of advancing.

  ## Fragility: bookkeeping discipline

  This table is a hand-rolled materialized view of the `keyrings` table.
  It can drift if anyone writes a keyring row without also calling the
  matching CAS operation here. The chain head is *not* derived at read
  time — it's a stored pointer maintained by the dispatch path.

  Conceptually cleaner would have been a regular Postgres view computing
  the head as "any keyring not pointed at by another's `supersedes_uri`."
  We chose the CAS table because it gives synchronous fork detection
  (the second writer's UPDATE affects zero rows, `:fork_detected` emits
  the SSE inline). A view-based approach would push fork detection into
  a separate scan or pre-insert EXISTS check, with race windows.

  The discipline that keeps this table honest:

    * Every `keyrings` INSERT must be paired with a call to `create/3` or
      `advance/4` on this module. `OpakeIndexer.Firehose.dispatch` is the
      only sanctioned writer; new code paths that touch `keyrings`
      directly will silently rot the chain head.
    * Tests that fixture keyring rows must either route through the
      firehose path (preferred — single source of truth) or explicitly
      call the matching chain-head update.
    * Backfill goes through `OpakeIndexer.Firehose.process_message/2` for
      this reason — see `OpakeIndexer.Backfill`.

  If you find yourself writing direct `Repo.insert(%Keyring{})`
  somewhere, that's a smell — either route through dispatch, or accept
  that the chain head will be wrong until the next supersede.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.KeyringChain

  @doc """
  Create a fresh keyring chain. Only called for genesis records. Returns
  `:already_exists` if the workspace already has a chain head — a
  second genesis with the same workspace_id is an indexer-level bug or
  a malformed record.
  """
  @spec create(String.t(), String.t(), String.t()) ::
          {:ok, KeyringChain.t()} | :already_exists | {:error, Ecto.Changeset.t()}
  def create(workspace_id, head_uri, head_cid) do
    %KeyringChain{}
    |> KeyringChain.changeset(%{
      workspace_id: workspace_id,
      head_uri: head_uri,
      head_cid: head_cid,
      updated_at: DateTime.utc_now()
    })
    |> Repo.insert(on_conflict: :nothing, conflict_target: :workspace_id)
    |> case do
      {:ok, %KeyringChain{workspace_id: ^workspace_id} = c} = ok ->
        # `on_conflict: :nothing` returns the unchanged existing row on
        # conflict — detect by checking head_uri didn't move to ours.
        if c.head_uri == head_uri, do: ok, else: :already_exists

      other ->
        other
    end
  end

  @doc """
  Advance the chain head. Compare-and-set on `expected_prior_uri`.
  Returns `{:ok, new_chain}` on success, `:fork_detected` if the
  expected prior doesn't match, or `:no_chain` if no chain exists.
  """
  @spec advance(String.t(), String.t(), String.t(), String.t()) ::
          {:ok, KeyringChain.t()} | :fork_detected | :no_chain
  def advance(workspace_id, head_uri, head_cid, expected_prior_uri) do
    now = DateTime.utc_now()

    {updated, _} =
      from(c in KeyringChain,
        where: c.workspace_id == ^workspace_id and c.head_uri == ^expected_prior_uri
      )
      |> Repo.update_all(set: [head_uri: head_uri, head_cid: head_cid, updated_at: now])

    case updated do
      1 ->
        {:ok, Repo.get(KeyringChain, workspace_id)}

      0 ->
        case Repo.get(KeyringChain, workspace_id) do
          nil -> :no_chain
          _ -> :fork_detected
        end
    end
  end

  @spec get(String.t()) :: KeyringChain.t() | nil
  def get(workspace_id), do: Repo.get(KeyringChain, workspace_id)

  @spec delete(String.t()) :: :ok
  def delete(workspace_id) do
    from(c in KeyringChain, where: c.workspace_id == ^workspace_id)
    |> Repo.delete_all()

    :ok
  end
end
