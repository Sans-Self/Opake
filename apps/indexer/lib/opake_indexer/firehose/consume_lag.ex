defmodule OpakeIndexer.Firehose.ConsumeLag do
  @moduledoc """
  Measures the indexer's own consume lag: the delta between an event's
  firehose `time_us` and the wall-clock moment the indexer processes it.

  Every consumed event contributes one sample. Samples land in a rolling
  power-of-two histogram (bucket upper bounds ~1ms to ~10m). Once per
  interval the accumulated histogram is summarized (p50/p95/p99, count,
  max) and logged as a structured line, then reset — so the reported
  distribution is per-interval, not lifetime.

  Sample output:

      [ConsumeLag] samples=1247 p50=8ms p95=512ms p99=2.0s max=4.0s \
      cursor=2026-07-12T10:11:12.345678Z

  An idle interval reports no samples rather than a growing lag figure —
  lag is measured per consumed event, so a window with no events simply
  has an empty histogram:

      [ConsumeLag] samples=0 (idle) cursor=2026-07-12T10:11:12.345678Z

  The `cursor=` timestamp is the last cursor the indexer persisted. It
  lets an operator distinguish idle (no samples, fresh cursor) from
  stalled (no samples, stale cursor while writes are known to be flowing).

  Started conditionally alongside the consumer (only when
  `:indexer_enabled`). The bucketing and percentile logic are pure
  functions so they can be unit-tested without the GenServer or a clock.
  """

  use GenServer
  require Logger

  alias OpakeIndexer.Firehose.State

  @default_interval_ms 60_000

  # Bucket upper bounds are 2^0..2^@max_exp milliseconds. 2^0 = 1ms is the
  # floor (sub-millisecond and clock-skew-negative lags clamp here); 2^20 ≈
  # 17.5m is the ceiling, comfortably above the ~10m range the contract
  # asks for so a pathological tail still lands in a real bucket rather
  # than overflowing.
  @max_exp 20

  @type histogram :: %{non_neg_integer() => pos_integer()}
  @type summary :: %{
          count: non_neg_integer(),
          p50: non_neg_integer() | nil,
          p95: non_neg_integer() | nil,
          p99: non_neg_integer() | nil,
          max: non_neg_integer() | nil
        }

  # -- Public API --

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc """
  Record one consume-lag sample from an event's processing.

  `now` is the wall-clock time the indexer began processing the event and
  `time_us` is the event's firehose timestamp (microseconds since epoch).
  A no-op cast when the process is not running, so callers on the hot path
  need no conditional. Negative lag (clock skew) clamps to the lowest
  bucket rather than crashing.
  """
  @spec record_lag(DateTime.t(), integer()) :: :ok
  def record_lag(now, time_us) when is_integer(time_us) do
    lag_us = DateTime.to_unix(now, :microsecond) - time_us
    GenServer.cast(__MODULE__, {:sample, lag_us})
  end

  @doc """
  Force an immediate summary log line and reset the interval histogram.
  Returns the summary that was logged. Useful in tests and ad-hoc
  inspection.
  """
  @spec tick() :: summary()
  def tick, do: GenServer.call(__MODULE__, :tick)

  # -- Pure histogram functions (unit-tested directly) --

  @doc """
  The bucket upper bounds in milliseconds: `[1, 2, 4, ..., 2^@max_exp]`.
  """
  @spec bucket_bounds() :: [pos_integer()]
  def bucket_bounds, do: Enum.map(0..@max_exp, &Integer.pow(2, &1))

  @doc """
  The histogram bucket index for a lag in milliseconds. Bucket `i` holds
  lags in `(2^(i-1), 2^i]` ms; the representative value of the bucket is
  its upper bound `2^i`. Lags ≤ 1ms (including clamped-negative skew) land
  in bucket 0; lags above the top bound saturate at `@max_exp`.
  """
  @spec bucket_index(integer()) :: non_neg_integer()
  def bucket_index(lag_ms) when is_integer(lag_ms) and lag_ms <= 1, do: 0

  def bucket_index(lag_ms) when is_integer(lag_ms) do
    min(ceil_log2(lag_ms), @max_exp)
  end

  @doc """
  Fold one lag sample (milliseconds) into a histogram.
  """
  @spec record_sample(histogram(), integer()) :: histogram()
  def record_sample(histogram, lag_ms) when is_integer(lag_ms) do
    Map.update(histogram, bucket_index(lag_ms), 1, &(&1 + 1))
  end

  @doc """
  Summarize a histogram into sample count and percentile/max bucket values
  (milliseconds). An empty histogram reports `count: 0` and `nil`
  percentiles — never a fabricated or growing lag figure.
  """
  @spec summarize(histogram()) :: summary()
  def summarize(histogram) do
    count = histogram |> Map.values() |> Enum.sum()

    if count == 0 do
      %{count: 0, p50: nil, p95: nil, p99: nil, max: nil}
    else
      %{
        count: count,
        p50: percentile(histogram, count, 0.50),
        p95: percentile(histogram, count, 0.95),
        p99: percentile(histogram, count, 0.99),
        max: max_bucket_value(histogram)
      }
    end
  end

  @doc """
  Format a summary plus the last-consumed cursor timestamp into the
  structured log line body (the text after the `[ConsumeLag] ` tag).
  """
  @spec format_summary(summary(), integer() | nil) :: String.t()
  def format_summary(%{count: 0}, cursor_time_us) do
    "samples=0 (idle) cursor=#{format_cursor(cursor_time_us)}"
  end

  def format_summary(summary, cursor_time_us) do
    "samples=#{summary.count} " <>
      "p50=#{format_ms(summary.p50)} " <>
      "p95=#{format_ms(summary.p95)} " <>
      "p99=#{format_ms(summary.p99)} " <>
      "max=#{format_ms(summary.max)} " <>
      "cursor=#{format_cursor(cursor_time_us)}"
  end

  # -- GenServer callbacks --

  @impl true
  def init(opts) do
    interval_ms = Keyword.get(opts, :interval_ms, @default_interval_ms)
    schedule(interval_ms)
    {:ok, %{interval_ms: interval_ms, histogram: %{}}}
  end

  @impl true
  def handle_cast({:sample, lag_us}, state) do
    lag_ms = max(lag_us, 0) |> div(1000)
    {:noreply, %{state | histogram: record_sample(state.histogram, lag_ms)}}
  end

  @impl true
  def handle_info(:report, state) do
    _summary = report(state.histogram)
    schedule(state.interval_ms)
    {:noreply, %{state | histogram: %{}}}
  end

  @impl true
  def handle_call(:tick, _from, state) do
    summary = report(state.histogram)
    {:reply, summary, %{state | histogram: %{}}}
  end

  # -- Internal --

  defp report(histogram) do
    summary = summarize(histogram)
    Logger.info("[ConsumeLag] " <> format_summary(summary, State.last_cursor_time_us()))
    summary
  end

  defp schedule(interval_ms) do
    Process.send_after(self(), :report, interval_ms)
  end

  # Smallest e such that 2^e >= n, for n > 1.
  defp ceil_log2(n) do
    Enum.find(1..64, fn e -> Integer.pow(2, e) >= n end)
  end

  # The percentile bucket's representative value: the upper bound of the
  # lowest bucket whose cumulative count reaches ⌈p·count⌉. Reported in ms.
  defp percentile(histogram, count, p) do
    target = ceil(p * count)

    histogram
    |> Enum.sort_by(fn {index, _} -> index end)
    |> Enum.reduce_while(0, fn {index, bucket_count}, cumulative ->
      cumulative = cumulative + bucket_count

      if cumulative >= target do
        {:halt, Integer.pow(2, index)}
      else
        {:cont, cumulative}
      end
    end)
  end

  defp max_bucket_value(histogram) do
    histogram
    |> Map.keys()
    |> Enum.max()
    |> then(&Integer.pow(2, &1))
  end

  defp format_ms(ms) when ms < 1000, do: "#{ms}ms"

  defp format_ms(ms) when ms < 60_000, do: "#{Float.round(ms / 1000, 1)}s"

  defp format_ms(ms), do: "#{Float.round(ms / 60_000, 1)}m"

  defp format_cursor(nil), do: "unknown"

  defp format_cursor(time_us) when is_integer(time_us) do
    case DateTime.from_unix(time_us, :microsecond) do
      {:ok, dt} -> DateTime.to_iso8601(dt)
      {:error, _} -> "unknown"
    end
  end
end
