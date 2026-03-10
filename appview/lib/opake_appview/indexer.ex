defmodule OpakeAppview.Indexer do
  @moduledoc """
  Dispatches parsed Jetstream events to the appropriate query module.
  Tracks indexer connection state via ETS (read by the health endpoint)
  and saves the cursor to Postgres every 100 events.
  """

  require Logger

  alias OpakeAppview.Queries.{CursorQueries, GrantQueries, KeyringQueries}
  alias OpakeAppview.Jetstream.Event

  @cursor_save_interval 100
  @state_table :indexer_state

  def init_state do
    :ets.new(@state_table, [:named_table, :set, :public, read_concurrency: true])
    :ets.insert(@state_table, {:connected, false})
  end

  def set_connected(connected) when is_boolean(connected) do
    :ets.insert(@state_table, {:connected, connected})
  end

  def connected? do
    case :ets.lookup(@state_table, :connected) do
      [{:connected, val}] -> val
      [] -> false
    end
  end

  def process_message(json, event_count) do
    case Event.parse(json) do
      {:upsert_grant, attrs} ->
        handle_upsert_grant(attrs)
        maybe_save_cursor(attrs.time_us, event_count + 1)

      {:delete_grant, %{uri: uri, time_us: time_us}} ->
        GrantQueries.delete_grant(uri)
        maybe_save_cursor(time_us, event_count + 1)

      {:upsert_keyring, attrs} ->
        handle_upsert_keyring(attrs)
        maybe_save_cursor(attrs.time_us, event_count + 1)

      {:delete_keyring, %{uri: uri, time_us: time_us}} ->
        KeyringQueries.delete_keyring(uri)
        maybe_save_cursor(time_us, event_count + 1)

      :ignore ->
        event_count
    end
  end

  defp handle_upsert_grant(attrs) do
    now = DateTime.utc_now()

    case GrantQueries.upsert_grant(%{
           uri: attrs.uri,
           owner_did: attrs.owner_did,
           recipient_did: attrs.recipient_did,
           document_uri: attrs.document_uri,
           created_at: attrs.created_at,
           indexed_at: now
         }) do
      {:ok, _} -> :ok
      {:error, changeset} -> Logger.warning("Failed to upsert grant #{attrs.uri}: #{inspect(changeset)}")
    end
  end

  defp handle_upsert_keyring(attrs) do
    case KeyringQueries.upsert_keyring(attrs.uri, attrs.owner_did, attrs.member_dids) do
      {:ok, _} -> :ok
      {:error, reason} -> Logger.warning("Failed to upsert keyring #{attrs.uri}: #{inspect(reason)}")
    end
  end

  defp maybe_save_cursor(time_us, event_count) do
    if rem(event_count, @cursor_save_interval) == 0 do
      CursorQueries.save_cursor(time_us)
      Logger.debug("Saved cursor at #{time_us} (#{event_count} events)")
    end

    event_count
  end
end
