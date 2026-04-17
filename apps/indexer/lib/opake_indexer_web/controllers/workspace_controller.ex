defmodule OpakeIndexerWeb.WorkspaceController do
  @moduledoc """
  Workspace-scoped endpoints. All actions require the caller to be a member
  of the keyring identified by the `?keyring=` parameter. Returns documents,
  directories, and delta-sync data for a workspace.
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Queries.{
    DirectoryQueries,
    DocumentQueries,
    DocumentUpdateQueries,
    KeyringQueries
  }

  import OpakeIndexerWeb.TreeHelpers
  import OpakeIndexerWeb.PaginationHelpers

  @spec snapshot(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def snapshot(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, keyring_uri} <- require_keyring(params),
         :ok <- check_membership(keyring_uri, did) do
      {directories, documents} = DirectoryQueries.workspace_tree(keyring_uri)
      proposals = DirectoryQueries.member_directory_updates(keyring_uri)
      keyring_proposals = KeyringQueries.member_keyring_updates(keyring_uri)
      document_proposals = DocumentUpdateQueries.member_document_updates(keyring_uri)
      server_time = DateTime.utc_now()

      json(
        conn,
        format_tree_response(directories, documents, server_time, proposals)
        |> Map.put("keyringProposals", Enum.map(keyring_proposals, &format_keyring_proposal/1))
        |> Map.put(
          "documentProposals",
          Enum.map(document_proposals, &format_document_proposal/1)
        )
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

    with {:ok, keyring_uri} <- require_keyring(params),
         :ok <- check_membership(keyring_uri, did),
         {:ok, since} <- parse_since(params) do
      {directories, documents} = DirectoryQueries.workspace_changes_since(keyring_uri, since)
      proposals = DirectoryQueries.member_directory_updates(keyring_uri)
      keyring_proposals = KeyringQueries.member_keyring_updates(keyring_uri)
      document_proposals = DocumentUpdateQueries.member_document_updates(keyring_uri)
      server_time = DateTime.utc_now()

      json(
        conn,
        format_tree_response(directories, documents, server_time, proposals)
        |> Map.put("keyringProposals", Enum.map(keyring_proposals, &format_keyring_proposal/1))
        |> Map.put(
          "documentProposals",
          Enum.map(document_proposals, &format_document_proposal/1)
        )
      )
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  # -- Legacy endpoints (used by web app, will migrate to snapshot/sync) --

  @spec documents(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def documents(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, keyring_uri} <- require_keyring(params),
         :ok <- check_membership(keyring_uri, did) do
      docs = DocumentQueries.list_documents(keyring_uri)

      response = %{
        documents:
          Enum.map(docs, fn d ->
            %{
              document_uri: d.document_uri,
              keyring_uri: d.keyring_uri,
              owner_did: d.owner_did,
              rotation: d.rotation,
              indexed_at: DateTime.to_iso8601(d.indexed_at)
            }
          end)
      }

      json(conn, response)
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  @spec updates(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def updates(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]
      document_uri = params["document"]

      {updates, next_cursor} =
        if is_binary(document_uri) and byte_size(document_uri) > 0 do
          DocumentUpdateQueries.list_document_updates(document_uri, limit: limit, cursor: cursor)
        else
          DocumentUpdateQueries.list_updates_for_owner(did, limit: limit, cursor: cursor)
        end

      response =
        %{
          updates:
            Enum.map(updates, fn u ->
              %{
                uri: u.uri,
                document_uri: u.document_uri,
                author_did: u.author_did,
                supersedes_uri: u.supersedes_uri,
                indexed_at: DateTime.to_iso8601(u.indexed_at)
              }
            end)
        }
        |> maybe_put_cursor(next_cursor)

      json(conn, response)
    else
      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  @spec directory_updates(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def directory_updates(conn, params) do
    did = conn.assigns.authenticated_did

    with {:ok, keyring_uri} <- require_keyring(params),
         :ok <- check_membership(keyring_uri, did),
         {:ok, limit} <- parse_limit(params) do
      cursor = params["cursor"]

      {updates, next_cursor} =
        DirectoryQueries.list_directory_updates(keyring_uri, limit: limit, cursor: cursor)

      response =
        %{
          directory_updates:
            Enum.map(updates, fn u ->
              %{
                uri: u.uri,
                keyring_uri: u.keyring_uri,
                author_did: u.author_did,
                action_type: u.action_type,
                directory_uri: u.directory_uri,
                entry_uri: u.entry_uri,
                indexed_at: DateTime.to_iso8601(u.indexed_at)
              }
            end)
        }
        |> maybe_put_cursor(next_cursor)

      json(conn, response)
    else
      {:error, status, message} ->
        conn |> put_status(status) |> json(%{error: message})

      {:error, message} ->
        conn |> put_status(400) |> json(%{error: message})
    end
  end

  # -- Helpers --

  @spec require_keyring(map()) :: {:ok, String.t()} | {:error, String.t()}
  defp require_keyring(%{"keyring" => uri}) when is_binary(uri) and byte_size(uri) > 0 do
    {:ok, uri}
  end

  defp require_keyring(_), do: {:error, "keyring parameter is required"}

  @spec check_membership(String.t(), String.t()) :: :ok | {:error, non_neg_integer(), String.t()}
  defp check_membership(keyring_uri, did) do
    if KeyringQueries.is_member?(keyring_uri, did) do
      :ok
    else
      {:error, 403, "not a member of this workspace"}
    end
  end
end
