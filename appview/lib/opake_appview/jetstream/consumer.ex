defmodule OpakeAppview.Jetstream.Consumer do
  @moduledoc """
  WebSocket client that subscribes to the Jetstream firehose for
  `app.opake.grant`, `app.opake.keyring`, `app.opake.directory`, and
  `app.opake.document` events. Resumes from the last saved cursor on startup.
  Reconnects with exponential backoff (1s → 60s max). Started conditionally —
  only when `:indexer_enabled` is true.
  """

  use WebSockex
  require Logger

  alias OpakeAppview.Indexer
  alias OpakeAppview.Queries.CursorQueries

  @initial_backoff_ms 1_000
  @max_backoff_ms 60_000
  @wanted_collections [
    "app.opake.grant",
    "app.opake.keyring",
    "app.opake.document",
    "app.opake.documentUpdate",
    "app.opake.keyringUpdate",
    "app.opake.directory",
    "app.opake.directoryUpdate"
  ]

  defstruct [:event_count, :backoff_ms]

  def start_link(_opts) do
    state = %__MODULE__{event_count: 0, backoff_ms: @initial_backoff_ms}

    url = build_subscription_url()
    log_cursor_lag()
    Logger.info("Jetstream connecting to #{url}")

    WebSockex.start_link(url, __MODULE__, state, name: __MODULE__)
  end

  @impl true
  def handle_connect(_conn, state) do
    Logger.info("Jetstream connected")
    Indexer.set_connected(true)
    {:ok, %{state | backoff_ms: @initial_backoff_ms}}
  end

  @impl true
  def handle_frame({:text, msg}, state) do
    event_count = Indexer.process_message(msg, state.event_count)
    {:ok, %{state | event_count: event_count}}
  end

  def handle_frame(_frame, state), do: {:ok, state}

  @impl true
  def handle_disconnect(%{reason: reason}, state) do
    Indexer.set_connected(false)

    Logger.warning(
      "Jetstream disconnected: #{inspect(reason)}, reconnecting in #{state.backoff_ms}ms"
    )

    Process.sleep(state.backoff_ms)

    next_backoff = min(state.backoff_ms * 2, @max_backoff_ms)
    {:reconnect, %{state | backoff_ms: next_backoff, event_count: 0}}
  end

  @impl true
  def handle_info(_msg, state), do: {:ok, state}

  defp log_cursor_lag do
    case CursorQueries.load_cursor() do
      %{time_us: time_us} ->
        cursor_dt = DateTime.from_unix!(time_us, :microsecond)
        lag_seconds = DateTime.diff(DateTime.utc_now(), cursor_dt, :second)

        lag_label =
          cond do
            lag_seconds < 60 -> "#{lag_seconds}s"
            lag_seconds < 3600 -> "#{div(lag_seconds, 60)}m"
            lag_seconds < 86400 -> "#{Float.round(lag_seconds / 3600, 1)}h"
            true -> "#{Float.round(lag_seconds / 86400, 1)}d"
          end

        Logger.info("Cursor is #{lag_label} behind (#{cursor_dt})")

      nil ->
        Logger.info("No cursor found, starting from live")
    end
  end

  defp build_subscription_url do
    base_url = jetstream_url()
    cursor = CursorQueries.load_cursor()

    collection_params =
      @wanted_collections
      |> Enum.map(&"wantedCollections=#{&1}")
      |> Enum.join("&")

    cursor_param =
      case cursor do
        %{time_us: time_us} -> "&cursor=#{time_us}"
        nil -> ""
      end

    "#{base_url}?#{collection_params}#{cursor_param}"
  end

  defp jetstream_url do
    Application.fetch_env!(:opake_appview, :jetstream_url)
  end
end
