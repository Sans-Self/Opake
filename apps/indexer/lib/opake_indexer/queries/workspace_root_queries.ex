defmodule OpakeIndexer.Queries.WorkspaceRootQueries do
  @moduledoc """
  Bookkeeping for workspace root directory chain heads. One row per
  workspace. Subtree chains aren't tracked separately — clients walk
  listing entries down from the root, which the cascade keeps fresh.

  `advance/4` follows the same compare-and-set pattern as keyring chains
  so forks can be detected.

  ## Fragility: bookkeeping discipline

  Same shape as `KeyringChainQueries` — see that module's docstring for
  the full reasoning. Briefly: this is a hand-rolled materialized view
  of the workspace-root directory chain, maintained by the dispatch
  path. It drifts if anyone writes a workspace-root directory row
  without also calling `create/3` or `advance/4` here.

  Only `OpakeIndexer.Firehose.dispatch` should write workspace-root
  directories. Backfill routes through the same dispatch path.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.WorkspaceRoot

  @spec create(String.t(), String.t(), String.t()) ::
          {:ok, WorkspaceRoot.t()} | :already_exists | {:error, Ecto.Changeset.t()}
  def create(workspace_id, head_uri, head_cid) do
    %WorkspaceRoot{}
    |> WorkspaceRoot.changeset(%{
      workspace_id: workspace_id,
      head_uri: head_uri,
      head_cid: head_cid,
      updated_at: DateTime.utc_now()
    })
    |> Repo.insert(on_conflict: :nothing, conflict_target: :workspace_id)
    |> case do
      {:ok, %WorkspaceRoot{workspace_id: ^workspace_id} = r} = ok ->
        if r.head_uri == head_uri, do: ok, else: :already_exists

      other ->
        other
    end
  end

  @spec advance(String.t(), String.t(), String.t(), String.t()) ::
          {:ok, WorkspaceRoot.t()} | :fork_detected | :no_chain
  def advance(workspace_id, head_uri, head_cid, expected_prior_uri) do
    now = DateTime.utc_now()

    {updated, _} =
      from(r in WorkspaceRoot,
        where: r.workspace_id == ^workspace_id and r.head_uri == ^expected_prior_uri
      )
      |> Repo.update_all(set: [head_uri: head_uri, head_cid: head_cid, updated_at: now])

    case updated do
      1 ->
        {:ok, Repo.get(WorkspaceRoot, workspace_id)}

      0 ->
        case Repo.get(WorkspaceRoot, workspace_id) do
          nil -> :no_chain
          _ -> :fork_detected
        end
    end
  end

  @spec get(String.t()) :: WorkspaceRoot.t() | nil
  def get(workspace_id), do: Repo.get(WorkspaceRoot, workspace_id)

  @spec delete(String.t()) :: :ok
  def delete(workspace_id) do
    from(r in WorkspaceRoot, where: r.workspace_id == ^workspace_id)
    |> Repo.delete_all()

    :ok
  end
end
