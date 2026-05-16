defmodule OpakeIndexerWeb.WorkspaceController do
  @moduledoc """
  Workspace-scoped endpoints. All actions require the caller to be a member
  of the workspace identified by the `?workspace_id=` parameter (= the
  genesis keyring URI).

  ## Endpoints

    * `GET /workspace/snapshot` — full tree (directories + documents)
    * `GET /workspace/sync?since=<iso8601>` — delta tree since timestamp
    * `GET /workspace/chain-head` — current keyring + workspace-root heads
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.{ChainHeadQueries, RecordQueries}

  import OpakeIndexerWeb.TreeHelpers

  @spec snapshot(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def snapshot(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, workspace_id} <- require_workspace_id(params),
         :ok <- check_membership(workspace_id, did) do
      {directories, documents} = RecordQueries.workspace_tree(workspace_id)
      server_time = DateTime.utc_now()

      json(
        conn,
        format_tree_response(directories, documents, server_time)
        |> Map.put(:workspace_id, workspace_id)
      )
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  @spec sync(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def sync(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, workspace_id} <- require_workspace_id(params),
         :ok <- check_membership(workspace_id, did),
         {:ok, since} <- parse_since(params) do
      {directories, documents} = RecordQueries.workspace_changes_since(workspace_id, since)

      server_time = DateTime.utc_now()

      json(
        conn,
        format_tree_response(directories, documents, server_time)
        |> Map.put(:workspace_id, workspace_id)
      )
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  @doc """
  Return the workspace's current chain head pointers (keyring + root
  directory). Clients use this to discover what URI to point `supersedes`
  at when writing the next mutation.
  """
  @spec chain_head(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def chain_head(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, workspace_id} <- require_workspace_id(params),
         :ok <- check_membership(workspace_id, did) do
      keyring = ChainHeadQueries.get(workspace_id, "keyring")
      root = ChainHeadQueries.get(workspace_id, "workspace_root")

      json(conn, %{
        workspace_id: workspace_id,
        keyring: keyring && %{head_uri: keyring.head_uri, head_cid: keyring.head_cid},
        root_directory: root && %{head_uri: root.head_uri, head_cid: root.head_cid}
      })
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  # -- Helpers --------------------------------------------------------

  @spec require_workspace_id(map()) :: {:ok, String.t()} | {:error, String.t()}
  defp require_workspace_id(%{"workspace_id" => id})
       when is_binary(id) and byte_size(id) > 0 do
    {:ok, id}
  end

  defp require_workspace_id(%{"workspaceId" => id})
       when is_binary(id) and byte_size(id) > 0 do
    {:ok, id}
  end

  defp require_workspace_id(_), do: {:error, "workspace_id parameter is required"}

  @spec check_membership(String.t(), String.t()) :: :ok | {:error, non_neg_integer(), String.t()}
  defp check_membership(workspace_id, did) do
    if RecordQueries.is_member?(workspace_id, did) do
      :ok
    else
      {:error, 403, "not a member of this workspace"}
    end
  end
end
