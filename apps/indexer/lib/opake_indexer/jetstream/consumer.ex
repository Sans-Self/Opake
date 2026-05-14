defmodule OpakeIndexer.Jetstream.Consumer do
  @moduledoc """
  WebSocket client that subscribes to a Jetstream firehose endpoint and
  routes every received frame through `OpakeIndexer.Firehose`.

  ## Subscription mode

  Controlled by the `:firehose_mode` config key:

    * `:full` (default in dev/prod) — no `wantedCollections` filter,
      receive every commit Jetstream emits. The parser ignores anything
      that isn't `app.opake.*`. This gives you proof-of-life logs even
      when no opake events are happening, which is the dev experience
      we actually want.

    * `:opake_only` — only the five `app.opake.*` collections, server-
      side filtered. Use this for low-bandwidth deployments or when you
      need a fast cold-start catch-up after extended downtime.

    * `{:custom, [collection_strings]}` — explicit collection list. Test
      env uses `{:custom, []}` so unit tests don't accidentally hit
      Jetstream.

  ## Compression

  Controlled by the `:compression` config key:

    * `:zstd` (default) — request zstd-compressed frames via the
      `?compress=true` query param. Cuts wire bandwidth by ~56%
      compared to the raw JSON firehose. Frames arrive as binary
      websocket messages and are decompressed via
      `OpakeIndexer.Jetstream.Compression` using a vendored dictionary.

    * `:none` — request raw JSON frames. Use only if zstd decoding is
      somehow broken in your environment, or for debugging.

  Resumes from the last saved cursor on startup. Reconnects with
  exponential backoff (1s → 60s max). Started conditionally — only when
  `:indexer_enabled` is true.
  """

  use WebSockex
  require Logger

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Firehose.State
  alias OpakeIndexer.Jetstream.Compression
  alias OpakeIndexer.Queries.CursorQueries

  @initial_backoff_ms 1_000
  @max_backoff_ms 60_000

  @opake_collections [
    "app.opake.grant",
    "app.opake.keyring",
    "app.opake.document",
    "app.opake.directory",
    "app.opake.accountConfig"
  ]

  defstruct [
    :event_count,
    :backoff_ms,
    :compression,
    :first_frame_validated,
    :decompression_context
  ]

  def start_link(_opts) do
    compression = compression_mode()

    decompression_context =
      if compression == :zstd do
        Compression.new_context()
      else
        nil
      end

    state = %__MODULE__{
      event_count: 0,
      backoff_ms: @initial_backoff_ms,
      compression: compression,
      first_frame_validated: false,
      decompression_context: decompression_context
    }

    url = build_subscription_url(state.compression)
    log_startup(url, state.compression)

    WebSockex.start_link(url, __MODULE__, state, name: __MODULE__)
  end

  @impl true
  def handle_connect(_conn, state) do
    Logger.info("[Jetstream] connected")
    State.set_connected(true)
    # Reset first-frame validation after every reconnect: dict drift could
    # appear at any reconnect window if bsky pushed a new dictionary.
    {:ok, %{state | backoff_ms: @initial_backoff_ms, first_frame_validated: false}}
  end

  # -- Frame handling --

  # Compressed binary frame (zstd path).
  @impl true
  def handle_frame({:binary, msg}, %__MODULE__{compression: :zstd} = state) do
    state =
      if state.first_frame_validated do
        state
      else
        # Crashes the process loudly on dict mismatch — way better than
        # mysterious decode failures downstream. The supervisor will
        # restart us with the standard backoff.
        Compression.validate_frame!(msg)
        Logger.info("[Jetstream] dict id validated against incoming frame")
        %{state | first_frame_validated: true}
      end

    case Compression.decompress(state.decompression_context, msg) do
      {:ok, json} ->
        event_count = Firehose.process_message(json, state.event_count)
        {:ok, %{state | event_count: event_count}}

      {:error, reason} ->
        Logger.error("[Jetstream] zstd decompress failed: #{inspect(reason)}")
        {:close, state}
    end
  end

  # Plain text frame (uncompressed path, or some servers send text even
  # when compression is requested for control frames).
  @impl true
  def handle_frame({:text, msg}, state) do
    event_count = Firehose.process_message(msg, state.event_count)
    {:ok, %{state | event_count: event_count}}
  end

  # If we requested no compression but get a binary frame anyway, treat
  # it as JSON-as-binary and try to process it. Defensive — shouldn't
  # happen in practice.
  @impl true
  def handle_frame({:binary, msg}, %__MODULE__{compression: :none} = state) do
    Logger.warning("[Jetstream] unexpected binary frame in :none mode, treating as JSON")
    event_count = Firehose.process_message(msg, state.event_count)
    {:ok, %{state | event_count: event_count}}
  end

  def handle_frame(_frame, state), do: {:ok, state}

  @impl true
  def handle_disconnect(%{reason: reason}, state) do
    State.set_connected(false)

    Logger.warning(
      "[Jetstream] disconnected: #{inspect(reason)}, reconnecting in #{state.backoff_ms}ms"
    )

    Process.sleep(state.backoff_ms)

    next_backoff = min(state.backoff_ms * 2, @max_backoff_ms)

    {:reconnect,
     %{state | backoff_ms: next_backoff, event_count: 0, first_frame_validated: false}}
  end

  @impl true
  def handle_info(_msg, state), do: {:ok, state}

  # -- Startup logging --

  defp log_startup(url, compression) do
    mode = firehose_mode()

    Logger.info(
      "[Jetstream] mode=#{format_mode(mode)} compression=#{compression} connecting to #{url}"
    )

    log_cursor_lag()
  end

  defp format_mode(:full), do: "full (no server-side filter)"
  defp format_mode(:opake_only), do: "opake_only (#{length(@opake_collections)} collections)"

  defp format_mode({:custom, list}) when is_list(list),
    do: "custom (#{length(list)} collections)"

  defp log_cursor_lag do
    case CursorQueries.load_cursor() do
      %{time_us: time_us} ->
        cursor_dt = DateTime.from_unix!(time_us, :microsecond)
        lag_seconds = DateTime.diff(DateTime.utc_now(), cursor_dt, :second)
        Logger.info("[Jetstream] cursor is #{format_lag(lag_seconds)} behind (#{cursor_dt})")

      nil ->
        Logger.info("[Jetstream] no cursor found, starting from live")
    end
  end

  defp format_lag(secs) when secs < 60, do: "#{secs}s"
  defp format_lag(secs) when secs < 3600, do: "#{div(secs, 60)}m"
  defp format_lag(secs) when secs < 86_400, do: "#{Float.round(secs / 3600, 1)}h"
  defp format_lag(secs), do: "#{Float.round(secs / 86_400, 1)}d"

  # -- URL building --

  defp build_subscription_url(compression) do
    base_url = jetstream_url()
    cursor = CursorQueries.load_cursor()
    collections = collections_for(firehose_mode())

    params =
      collections
      |> Enum.map(&"wantedCollections=#{&1}")
      |> Enum.concat(cursor_param(cursor))
      |> Enum.concat(compression_param(compression))
      |> Enum.join("&")

    case params do
      "" -> base_url
      _ -> "#{base_url}?#{params}"
    end
  end

  defp collections_for(:full), do: []
  defp collections_for(:opake_only), do: @opake_collections
  defp collections_for({:custom, list}) when is_list(list), do: list

  defp cursor_param(%{time_us: time_us}), do: ["cursor=#{time_us}"]
  defp cursor_param(_), do: []

  defp compression_param(:zstd), do: ["compress=true"]
  defp compression_param(:none), do: []

  defp firehose_mode do
    Application.get_env(:opake_indexer, :firehose_mode, :full)
  end

  defp compression_mode do
    Application.get_env(:opake_indexer, :compression, :zstd)
  end

  defp jetstream_url do
    Application.fetch_env!(:opake_indexer, :jetstream_url)
  end
end
