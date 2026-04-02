defmodule OpakeAppview.Indexer do
  @moduledoc """
  Dispatches parsed Jetstream events to the appropriate query module.
  Tracks indexer connection state via ETS (read by the health endpoint)
  and saves the cursor to Postgres every 100 events.
  """

  require Logger

  alias OpakeAppview.Queries.{
    CursorQueries,
    DirectoryQueries,
    DocumentQueries,
    DocumentUpdateQueries,
    GrantQueries,
    KeyringQueries
  }

  alias OpakeAppview.Jetstream.Event

  @cursor_save_interval 1
  @state_table :indexer_state

  @spec init_state() :: :ets.table()
  def init_state do
    :ets.new(@state_table, [:named_table, :set, :public, read_concurrency: true])
    :ets.insert(@state_table, {:connected, false})
  end

  @spec set_connected(boolean()) :: true
  def set_connected(connected) when is_boolean(connected) do
    :ets.insert(@state_table, {:connected, connected})
  end

  @spec connected?() :: boolean()
  def connected? do
    case :ets.lookup(@state_table, :connected) do
      [{:connected, val}] -> val
      [] -> false
    end
  end

  @spec process_message(binary(), non_neg_integer()) :: non_neg_integer()
  def process_message(json, event_count) do
    now = DateTime.utc_now()

    case Event.parse(json) do
      :ignore ->
        # Still advance the counter so the cursor saves periodically.
        # Extract time_us from the raw JSON for cursor position.
        time_us = extract_time_us(json)
        maybe_save_cursor(time_us, event_count + 1)

      event ->
        dispatch(event, now, event_count)
    end
  end

  defp extract_time_us(json) do
    case Jason.decode(json) do
      {:ok, %{"time_us" => t}} when is_integer(t) -> t
      _ -> 0
    end
  end

  # -- Grant --

  defp dispatch({:upsert_grant, attrs}, now, event_count) do
    Logger.info(
      "Indexing grant upsert: #{attrs.uri} (owner=#{attrs.owner_did}, recipient=#{attrs.recipient_did})"
    )

    case GrantQueries.upsert_grant(%{
           uri: attrs.uri,
           owner_did: attrs.owner_did,
           recipient_did: attrs.recipient_did,
           document_uri: attrs.document_uri,
           created_at: attrs.created_at,
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, changeset} ->
        Logger.warning("Failed to upsert grant #{attrs.uri}: #{inspect(changeset)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch({:delete_grant, %{uri: uri, time_us: time_us}}, _now, event_count) do
    Logger.info("Indexing grant delete: #{uri}")
    GrantQueries.delete_grant(uri)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Keyring --

  defp dispatch({:upsert_keyring, attrs}, _now, event_count) do
    Logger.info(
      "Indexing keyring upsert: #{attrs.uri} (owner=#{attrs.owner_did}, members=#{length(attrs.member_entries)})"
    )

    case KeyringQueries.upsert_keyring(attrs.uri, attrs.owner_did, attrs.member_entries) do
      {:ok, _} ->
        :ok

      {:error, reason} ->
        Logger.warning("Failed to upsert keyring members #{attrs.uri}: #{inspect(reason)}")
    end

    case KeyringQueries.upsert_keyring_record(attrs) do
      {:ok, _} ->
        :ok

      {:error, reason} ->
        Logger.warning("Failed to upsert keyring record #{attrs.uri}: #{inspect(reason)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch({:delete_keyring, %{uri: uri, time_us: time_us}}, _now, event_count) do
    Logger.info("Indexing keyring delete: #{uri}")
    KeyringQueries.delete_keyring(uri)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Directory --

  defp dispatch({:upsert_directory, attrs}, now, event_count) do
    Logger.info("Indexing directory: #{attrs.directory_uri}")

    case DirectoryQueries.upsert_directory(%{
           directory_uri: attrs.directory_uri,
           keyring_uri: attrs.keyring_uri,
           owner_did: attrs.owner_did,
           entries: attrs.entries,
           encrypted_metadata: attrs.encrypted_metadata,
           key_wrapping: attrs.key_wrapping,
           deleted_at: nil,
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, cs} ->
        Logger.warning("Failed to upsert directory #{attrs.directory_uri}: #{inspect(cs)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch(
         {:delete_directory, %{directory_uri: directory_uri, time_us: time_us}},
         now,
         event_count
       ) do
    Logger.info("Indexing directory delete (soft): #{directory_uri}")
    DirectoryQueries.soft_delete_directory(directory_uri, now)
    maybe_save_cursor(time_us, event_count + 1)
  end

  defp dispatch({:upsert_document, attrs}, now, event_count) do
    Logger.info("Indexing document: #{attrs.document_uri}")

    case DocumentQueries.upsert_document(%{
           document_uri: attrs.document_uri,
           keyring_uri: attrs.keyring_uri,
           owner_did: attrs.owner_did,
           rotation: attrs.rotation,
           encrypted_metadata: attrs.encrypted_metadata,
           encryption: attrs.encryption,
           blob_ref: attrs.blob_ref,
           deleted_at: nil,
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, cs} ->
        Logger.warning("Failed to upsert document #{attrs.document_uri}: #{inspect(cs)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch(
         {:delete_document, %{document_uri: document_uri, time_us: time_us}},
         now,
         event_count
       ) do
    Logger.info("Indexing document delete (soft): #{document_uri}")
    DocumentQueries.soft_delete_document(document_uri, now)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Document updates --

  defp dispatch({:upsert_document_update, attrs}, now, event_count) do
    Logger.info("Indexing document update: #{attrs.uri} → #{attrs.document_uri}")

    case DocumentUpdateQueries.upsert_document_update(%{
           uri: attrs.uri,
           document_uri: attrs.document_uri,
           author_did: attrs.author_did,
           supersedes_uri: attrs.supersedes_uri,
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, cs} ->
        Logger.warning("Failed to upsert document update #{attrs.uri}: #{inspect(cs)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch({:delete_document_update, %{uri: uri, time_us: time_us}}, _now, event_count) do
    Logger.info("Indexing document update delete: #{uri}")
    DocumentUpdateQueries.delete_document_update(uri)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Directory updates --

  defp dispatch({:upsert_directory_update, attrs}, now, event_count) do
    Logger.info("Indexing directory update: #{attrs.uri} → #{attrs.keyring_uri}")

    case DirectoryQueries.upsert_directory_update(%{
           uri: attrs.uri,
           keyring_uri: attrs.keyring_uri,
           author_did: attrs.author_did,
           action_type: attrs.action_type,
           directory_uri: attrs.directory_uri,
           entry_uri: attrs.entry_uri,
           encrypted_metadata: attrs[:encrypted_metadata],
           source_directory_uri: attrs[:source_directory_uri],
           target_directory_uri: attrs[:target_directory_uri],
           parent_directory_uri: attrs[:parent_directory_uri],
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, cs} ->
        Logger.warning("Failed to upsert directory update #{attrs.uri}: #{inspect(cs)}")
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch({:delete_directory_update, %{uri: uri, time_us: time_us}}, _now, event_count) do
    Logger.info("Indexing directory update delete: #{uri}")
    DirectoryQueries.delete_directory_update(uri)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Keyring updates --

  defp dispatch({:upsert_keyring_update, attrs}, now, event_count) do
    Logger.info("Indexing keyring update: #{attrs.uri} → #{attrs.keyring_uri}")

    case KeyringQueries.upsert_keyring_update(%{
           uri: attrs.uri,
           keyring_uri: attrs.keyring_uri,
           author_did: attrs.author_did,
           action_type: attrs.action_type,
           member_did: attrs[:member_did],
           member_public_key: attrs[:member_public_key],
           role: attrs[:role],
           encrypted_metadata: attrs[:encrypted_metadata],
           indexed_at: now
         }) do
      {:ok, _} ->
        :ok

      {:error, cs} ->
        Logger.warning("Failed to upsert keyring update #{attrs.uri}: #{inspect(cs)}")
    end

    # Immediate visibility: "leave" removes the member from the AppView's index
    if attrs.action_type == "leave" do
      Logger.info("Keyring leave: #{attrs.author_did} left #{attrs.keyring_uri}")
      KeyringQueries.remove_member(attrs.keyring_uri, attrs.author_did)
    end

    maybe_save_cursor(attrs.time_us, event_count + 1)
  end

  defp dispatch({:delete_keyring_update, %{uri: uri, time_us: time_us}}, _now, event_count) do
    Logger.info("Indexing keyring update delete: #{uri}")
    KeyringQueries.delete_keyring_update(uri)
    maybe_save_cursor(time_us, event_count + 1)
  end

  # -- Cursor --

  defp maybe_save_cursor(time_us, event_count) do
    if rem(event_count, @cursor_save_interval) == 0 do
      CursorQueries.save_cursor(time_us)
      Logger.debug("Saved cursor at #{time_us} (#{event_count} events)")
    end

    event_count
  end
end
