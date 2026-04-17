defmodule OpakeIndexer.Firehose.Heartbeat do
  @moduledoc """
  Periodic liveness logger for the Jetstream indexer.

  Reads `OpakeIndexer.Firehose.State` every #{div(30_000, 1000)}s and logs a
  one-line summary so operators can verify the firehose is flowing without
  having to hit the health endpoint or wait for an opake-specific event.

  Sample output:

      [Heartbeat] connected=true events=1247 (indexed=7 ignored=1240) \
      last_event=1.2s ago cursor_lag=2s top=app.bsky.feed.post:1100,...

  Started conditionally alongside the consumer (only when
  `:indexer_enabled` is true). Reads-only — never mutates state.
  """

  use GenServer
  require Logger

  alias OpakeIndexer.Firehose.State

  @default_interval_ms 30_000
  # How many per-collection counts to include in the log line.
  @top_n 5

  # -- Public API --

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc """
  Force an immediate heartbeat log line. Useful for tests and for the
  `mix opake.tail` task. Returns the snapshot that was logged.
  """
  @spec tick() :: State.snapshot()
  def tick, do: GenServer.call(__MODULE__, :tick)

  # -- GenServer callbacks --

  @impl true
  def init(opts) do
    interval_ms = Keyword.get(opts, :interval_ms, @default_interval_ms)
    schedule(interval_ms)
    {:ok, %{interval_ms: interval_ms}}
  end

  @impl true
  def handle_info(:heartbeat, state) do
    log_snapshot(State.snapshot())
    schedule(state.interval_ms)
    {:noreply, state}
  end

  @impl true
  def handle_call(:tick, _from, state) do
    snapshot = State.snapshot()
    log_snapshot(snapshot)
    {:reply, snapshot, state}
  end

  # -- Internal --

  defp schedule(interval_ms) do
    Process.send_after(self(), :heartbeat, interval_ms)
  end

  defp log_snapshot(snapshot) do
    Logger.info("[Heartbeat] " <> format_snapshot(snapshot))
  end

  @doc false
  @spec format_snapshot(State.snapshot()) :: String.t()
  def format_snapshot(s) do
    [
      "connected=#{s.connected}",
      "events=#{s.total} (indexed=#{s.indexed} ignored=#{s.ignored})",
      "last_event=#{format_age(s.last_event_age_ms)}",
      "cursor_lag=#{format_cursor_lag(s.cursor_time_us)}",
      format_top_collections(s.per_collection)
    ]
    |> Enum.reject(&(&1 == ""))
    |> Enum.join(" ")
  end

  defp format_age(nil), do: "never"

  defp format_age(ms) when is_integer(ms) do
    cond do
      ms < 1000 -> "#{ms}ms ago"
      ms < 60_000 -> "#{Float.round(ms / 1000, 1)}s ago"
      ms < 3_600_000 -> "#{div(ms, 60_000)}m ago"
      true -> "#{Float.round(ms / 3_600_000, 1)}h ago"
    end
  end

  defp format_cursor_lag(nil), do: "unknown"

  defp format_cursor_lag(time_us) when is_integer(time_us) do
    now_us = DateTime.utc_now() |> DateTime.to_unix(:microsecond)
    lag_secs = div(now_us - time_us, 1_000_000)

    # Composite units (1m30s, 8h24m, 2d4h) rather than rounded decimals
    # so live-updating lag values visibly tick per minute when the
    # indexer is catching up — `8.4h` stayed `8.4h` for 20 minutes,
    # whereas `8h24m` ticks every minute.
    cond do
      lag_secs < 0 ->
        "0s"

      lag_secs < 60 ->
        "#{lag_secs}s"

      lag_secs < 3600 ->
        mins = div(lag_secs, 60)
        secs = rem(lag_secs, 60)
        if secs == 0, do: "#{mins}m", else: "#{mins}m#{secs}s"

      lag_secs < 86_400 ->
        hours = div(lag_secs, 3600)
        mins = div(rem(lag_secs, 3600), 60)
        if mins == 0, do: "#{hours}h", else: "#{hours}h#{mins}m"

      true ->
        days = div(lag_secs, 86_400)
        hours = div(rem(lag_secs, 86_400), 3600)
        if hours == 0, do: "#{days}d", else: "#{days}d#{hours}h"
    end
  end

  defp format_top_collections(counts) when map_size(counts) == 0, do: ""

  defp format_top_collections(counts) do
    top =
      counts
      |> Enum.sort_by(fn {_, n} -> -n end)
      |> Enum.take(@top_n)
      |> Enum.map_join(",", fn {name, n} -> "#{short(name)}:#{n}" end)

    "top=#{top}"
  end

  # Compress collection names for log readability:
  #   "app.opake.document"     -> "opake.document"
  #   "app.bsky.feed.post"     -> "bsky.feed.post"
  #   "chat.bsky.convo.message" -> "chat.bsky.convo.message"
  defp short("app." <> rest), do: rest
  defp short(other), do: other
end
