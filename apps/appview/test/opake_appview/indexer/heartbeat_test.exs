defmodule OpakeAppview.Indexer.HeartbeatTest do
  @moduledoc """
  Tests for the heartbeat log line formatter. The GenServer itself is
  trivial (timer + State.snapshot/0 + Logger.info), so we focus on
  format_snapshot/1 which is where the actual logic lives.
  """

  use ExUnit.Case, async: true

  alias OpakeAppview.Indexer.Heartbeat

  defp base_snapshot(overrides \\ %{}) do
    Map.merge(
      %{
        connected: true,
        total: 0,
        indexed: 0,
        ignored: 0,
        last_event_age_ms: nil,
        cursor_time_us: nil,
        cursor_saved_age_ms: nil,
        per_collection: %{}
      },
      overrides
    )
  end

  test "formats a quiet snapshot" do
    line = Heartbeat.format_snapshot(base_snapshot())

    assert line =~ "connected=true"
    assert line =~ "events=0 (indexed=0 ignored=0)"
    assert line =~ "last_event=never"
    assert line =~ "cursor_lag=unknown"
    refute line =~ "top="
  end

  test "formats a busy snapshot with top collections" do
    snapshot =
      base_snapshot(%{
        total: 1247,
        indexed: 7,
        ignored: 1240,
        last_event_age_ms: 1234,
        per_collection: %{
          "app.bsky.feed.post" => 1100,
          "app.bsky.feed.like" => 100,
          "app.opake.document" => 4,
          "app.opake.grant" => 3
        }
      })

    line = Heartbeat.format_snapshot(snapshot)

    assert line =~ "events=1247 (indexed=7 ignored=1240)"
    assert line =~ "last_event=1.2s ago"
    # bsky.feed.post is most populous, should appear first
    assert line =~ "top=bsky.feed.post:1100"
    # opake.* collections should appear with their short names
    assert line =~ "opake.document:4"
  end

  test "formats stale ages in human units" do
    assert Heartbeat.format_snapshot(base_snapshot(%{last_event_age_ms: 30})) =~
             "last_event=30ms ago"

    assert Heartbeat.format_snapshot(base_snapshot(%{last_event_age_ms: 5_000})) =~
             "last_event=5.0s ago"

    assert Heartbeat.format_snapshot(base_snapshot(%{last_event_age_ms: 90_000})) =~
             "last_event=1m ago"

    assert Heartbeat.format_snapshot(base_snapshot(%{last_event_age_ms: 5_400_000})) =~
             "last_event=1.5h ago"
  end

  test "cursor_lag rounds to seconds when current" do
    # Cursor is current → lag is 0s
    now_us = DateTime.utc_now() |> DateTime.to_unix(:microsecond)
    line = Heartbeat.format_snapshot(base_snapshot(%{cursor_time_us: now_us}))

    assert line =~ "cursor_lag=0s"
  end

  test "cursor_lag formats minute-granular lag under an hour" do
    ninety_seconds_ago =
      DateTime.utc_now()
      |> DateTime.add(-90, :second)
      |> DateTime.to_unix(:microsecond)

    line = Heartbeat.format_snapshot(base_snapshot(%{cursor_time_us: ninety_seconds_ago}))
    assert line =~ "cursor_lag=1m30s"
  end

  test "cursor_lag formats hours and minutes together" do
    # 8 hours 24 minutes ago = 30240 seconds
    lag_us =
      DateTime.utc_now()
      |> DateTime.add(-30_240, :second)
      |> DateTime.to_unix(:microsecond)

    line = Heartbeat.format_snapshot(base_snapshot(%{cursor_time_us: lag_us}))
    assert line =~ "cursor_lag=8h24m"
  end

  test "cursor_lag formats whole hours without minute suffix" do
    two_hours_ago =
      DateTime.utc_now()
      |> DateTime.add(-7200, :second)
      |> DateTime.to_unix(:microsecond)

    line = Heartbeat.format_snapshot(base_snapshot(%{cursor_time_us: two_hours_ago}))
    assert line =~ "cursor_lag=2h"
    refute line =~ "cursor_lag=2h0m"
  end

  test "cursor_lag formats days and hours together" do
    # 2 days 4 hours = 187200 seconds
    lag_us =
      DateTime.utc_now()
      |> DateTime.add(-187_200, :second)
      |> DateTime.to_unix(:microsecond)

    line = Heartbeat.format_snapshot(base_snapshot(%{cursor_time_us: lag_us}))
    assert line =~ "cursor_lag=2d4h"
  end
end
