defmodule OpakeIndexer.FirehoseCursorMonotonicTest do
  @moduledoc """
  Pipeline coverage for cursor monotonicity: the persisted Jetstream cursor
  advances only when an incoming event's `time_us` exceeds the last saved
  value, and an out-of-order (older) frame never regresses it.

  Touches the process-global cursor State ETS and the singleton cursor row,
  so it runs `async: false` and reseeds the State baseline in setup rather
  than trusting whatever a prior test left behind.
  """

  use OpakeIndexer.DataCase, async: false

  alias OpakeIndexer.Firehose
  alias OpakeIndexer.Firehose.State
  alias OpakeIndexer.Queries.CursorQueries

  @did "did:plc:cursor"

  # A non-ignored commit so dispatch runs; an unknown-record delete needs no
  # seeding and reaches the same cursor-save path as any consumed event.
  defp commit_json(time_us) do
    Jason.encode!(%{
      "kind" => "commit",
      "did" => @did,
      "time_us" => time_us,
      "commit" => %{
        "operation" => "delete",
        "collection" => "app.opake.document",
        "rkey" => "doc-#{time_us}"
      }
    })
  end

  setup do
    # Baseline the shared State cursor below the values this test drives so a
    # prior test's high-water mark can't suppress the first advance.
    State.record_cursor_save(0)
    :ok
  end

  # spec:indexer-consistency § The cursor is strictly monotonic
  test "a newer event advances the persisted cursor" do
    Firehose.process_message(commit_json(200), 0)

    assert %{time_us: 200} = CursorQueries.load_cursor()
  end

  # spec:indexer-consistency § The cursor is strictly monotonic
  test "an out-of-order older frame does not regress the cursor" do
    Firehose.process_message(commit_json(200), 0)
    assert %{time_us: 200} = CursorQueries.load_cursor()

    Firehose.process_message(commit_json(100), 1)

    assert %{time_us: 200} = CursorQueries.load_cursor()
  end
end
