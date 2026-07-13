defmodule OpakeIndexer.Firehose.ConsumeLagTest do
  @moduledoc """
  Unit tests for the consume-lag histogram. Bucketing, percentile
  extraction, and the idle-interval report are pure functions, so they are
  exercised directly without the GenServer or a clock.
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.Firehose.ConsumeLag

  defp histogram(lags), do: Enum.reduce(lags, %{}, &ConsumeLag.record_sample(&2, &1))

  # spec:indexer-consistency § The indexer measures its own consume lag
  test "lag values land in the expected power-of-two buckets" do
    # Bucket i covers (2^(i-1), 2^i] ms; representative value is 2^i.
    assert ConsumeLag.bucket_index(1) == 0
    assert ConsumeLag.bucket_index(2) == 1
    assert ConsumeLag.bucket_index(3) == 2
    assert ConsumeLag.bucket_index(4) == 2
    assert ConsumeLag.bucket_index(5) == 3
    assert ConsumeLag.bucket_index(1000) == 10
    assert ConsumeLag.bucket_index(1024) == 10
  end

  # spec:indexer-consistency § The indexer measures its own consume lag
  test "negative and sub-millisecond lag clamps to the lowest bucket" do
    # Clock skew must not crash or produce a negative index.
    assert ConsumeLag.bucket_index(-100) == 0
    assert ConsumeLag.bucket_index(0) == 0
    assert ConsumeLag.record_sample(%{}, -100) == %{0 => 1}
  end

  # spec:indexer-consistency § The indexer measures its own consume lag
  test "an extreme lag saturates at the top bucket rather than overflowing" do
    top = length(ConsumeLag.bucket_bounds()) - 1
    assert ConsumeLag.bucket_index(Integer.pow(2, 40)) == top
  end

  # spec:indexer-consistency § The indexer measures its own consume lag
  test "percentile extraction yields the expected p50/p95/p99 buckets" do
    hist =
      histogram(
        List.duplicate(8, 90) ++
          List.duplicate(500, 9) ++
          List.duplicate(4000, 1)
      )

    assert %{count: 100, p50: 8, p95: 512, p99: 512, max: 4096} = ConsumeLag.summarize(hist)
  end

  # spec:indexer-consistency § The indexer measures its own consume lag
  test "an idle interval reports zero samples, not a growing lag" do
    summary = ConsumeLag.summarize(%{})
    assert summary == %{count: 0, p50: nil, p95: nil, p99: nil, max: nil}

    line = ConsumeLag.format_summary(summary, nil)
    assert line =~ "samples=0"
    assert line =~ "idle"
    refute line =~ "p50="
    refute line =~ "p99="
  end

  test "the log line carries the last-consumed cursor timestamp" do
    hist = histogram([8, 8, 500])
    cursor_us = DateTime.to_unix(~U[2026-07-12 10:11:12.345678Z], :microsecond)

    line = ConsumeLag.format_summary(ConsumeLag.summarize(hist), cursor_us)

    assert line =~ "samples=3"
    assert line =~ "cursor=2026-07-12T10:11:12.345678Z"
  end
end
