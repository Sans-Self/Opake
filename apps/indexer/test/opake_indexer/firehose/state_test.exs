defmodule OpakeIndexer.Firehose.StateTest do
  @moduledoc """
  Unit tests for the indexer state ETS table. The table is initialized
  once by the application supervisor and shared across all tests, so
  these tests can't assume a clean slate — they assert on relative
  changes (delta from a snapshot) instead of absolute values.
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.Firehose.State

  test "bump_total/0 returns the new value" do
    before = State.counter(:counter_total)
    new_val = State.bump_total()

    assert new_val == before + 1
  end

  test "bump_collection/1 increments per-collection counters" do
    name = "test.bump.#{System.unique_integer([:positive])}"

    State.bump_collection(name)
    State.bump_collection(name)
    State.bump_collection(name)

    assert State.collection_counts()[name] == 3
  end

  test "bump_collection/1 ignores nil" do
    counts_before = State.collection_counts()
    assert State.bump_collection(nil) == 0
    assert State.collection_counts() == counts_before
  end

  test "mark_event_received/0 sets last_event_age_ms to ~0" do
    State.mark_event_received()
    age = State.last_event_age_ms()

    assert is_integer(age)
    # Should be very small — we just marked it
    assert age < 100
  end

  test "record_cursor_save/1 stores the time_us and resets the saved-age clock" do
    time_us = System.os_time(:microsecond)
    State.record_cursor_save(time_us)

    assert State.last_cursor_time_us() == time_us
    assert State.cursor_saved_age_ms() < 100
  end

  test "snapshot/0 returns a map with all fields" do
    State.bump_total()
    State.mark_event_received()

    snapshot = State.snapshot()

    assert is_boolean(snapshot.connected)
    assert is_integer(snapshot.total)
    assert is_integer(snapshot.indexed)
    assert is_integer(snapshot.ignored)
    assert is_integer(snapshot.last_event_age_ms)
    assert is_map(snapshot.per_collection)
  end
end
