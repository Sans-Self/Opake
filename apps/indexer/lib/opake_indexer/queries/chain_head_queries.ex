defmodule OpakeIndexer.Queries.ChainHeadQueries do
  @moduledoc """
  Compare-and-set helpers for `chain_heads`. One row per
  `(workspace_id, kind)`. The firehose dispatch is the only writer.

  `create/3` (genesis): inserts if no row exists; returns `:already_exists`
  if a different head is already pinned.

  `advance/4` (supersede): updates only if the current row's `head_uri`
  matches the supplied `prior_uri` — atomic compare-and-set. Returns
  `:fork_detected` if the prior URI doesn't match (another writer beat
  us), `:no_chain` if no chain exists yet.

  `rollback/3` (head delete): sets `head_uri = predecessor_uri` only if
  current head matches the deleted URI.

  These three functions are the only mutation surface for chain heads.
  """

  import Ecto.Query

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.ChainHead

  @type kind :: String.t()
  @type result :: {:ok, ChainHead.t()} | :already_exists | :fork_detected | :no_chain
  @type create_result :: {:ok, ChainHead.t()} | :already_exists | {:error, term()}
  @type advance_result :: {:ok, ChainHead.t()} | :fork_detected | :no_chain
  @type rollback_result :: {:ok, ChainHead.t()} | :no_chain | :head_mismatch

  @spec get(String.t(), kind()) :: ChainHead.t() | nil
  def get(workspace_id, kind) do
    Repo.get_by(ChainHead, workspace_id: workspace_id, kind: kind)
  end

  @spec create(String.t(), kind(), String.t(), String.t()) :: create_result()
  def create(workspace_id, kind, head_uri, head_cid) do
    now = DateTime.utc_now()

    attrs = %{
      workspace_id: workspace_id,
      kind: kind,
      head_uri: head_uri,
      head_cid: head_cid,
      updated_at: now
    }

    case Repo.insert(ChainHead.changeset(%ChainHead{}, attrs), on_conflict: :nothing) do
      {:ok, %ChainHead{head_uri: ^head_uri} = head} ->
        {:ok, head}

      {:ok, _stale} ->
        # `:nothing` returns a row with the supplied PK regardless of
        # whether the insert actually happened. Re-fetch and compare.
        case get(workspace_id, kind) do
          %ChainHead{head_uri: ^head_uri} = head -> {:ok, head}
          _ -> :already_exists
        end

      {:error, changeset} ->
        {:error, changeset}
    end
  end

  @spec advance(String.t(), kind(), String.t(), String.t(), String.t()) :: advance_result()
  def advance(workspace_id, kind, head_uri, head_cid, prior_uri) do
    now = DateTime.utc_now()

    {count, _} =
      from(c in ChainHead,
        where:
          c.workspace_id == ^workspace_id and
            c.kind == ^kind and
            c.head_uri == ^prior_uri
      )
      |> Repo.update_all(set: [head_uri: head_uri, head_cid: head_cid, updated_at: now])

    case count do
      1 ->
        {:ok, get(workspace_id, kind)}

      0 ->
        case get(workspace_id, kind) do
          nil -> :no_chain
          _ -> :fork_detected
        end
    end
  end

  @spec rollback(String.t(), kind(), String.t(), String.t(), String.t()) :: rollback_result()
  def rollback(workspace_id, kind, deleted_head_uri, predecessor_uri, predecessor_cid) do
    now = DateTime.utc_now()

    {count, _} =
      from(c in ChainHead,
        where:
          c.workspace_id == ^workspace_id and
            c.kind == ^kind and
            c.head_uri == ^deleted_head_uri
      )
      |> Repo.update_all(set: [head_uri: predecessor_uri, head_cid: predecessor_cid, updated_at: now])

    case count do
      1 ->
        {:ok, get(workspace_id, kind)}

      0 ->
        case get(workspace_id, kind) do
          nil -> :no_chain
          _ -> :head_mismatch
        end
    end
  end

  @doc """
  Delete the entire chain row. Used by `keyring:delete` cascade — when
  the workspace's keyring chain goes away, the workspace_root chain is
  no longer meaningful either.
  """
  @spec delete(String.t(), kind()) :: :ok
  def delete(workspace_id, kind) do
    from(c in ChainHead,
      where: c.workspace_id == ^workspace_id and c.kind == ^kind
    )
    |> Repo.delete_all()

    :ok
  end

  @doc """
  Delete every chain row for a workspace. Called when the entire
  workspace is being torn down (genesis keyring delete).
  """
  @spec delete_all(String.t()) :: :ok
  def delete_all(workspace_id) do
    from(c in ChainHead, where: c.workspace_id == ^workspace_id)
    |> Repo.delete_all()

    :ok
  end
end
