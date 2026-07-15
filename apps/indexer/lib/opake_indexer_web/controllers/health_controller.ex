defmodule OpakeIndexerWeb.HealthController do
  @moduledoc """
  Unauthenticated health endpoint. Surfaces enough indexer state for an
  operator (or a monitoring probe) to distinguish "WS connected and
  flowing" from "WS connected but idle" from "WS dead". Row counts are
  intentionally omitted — those are internal metrics, not public signals.

  ## Response shape

      {
        "indexer_connected": true,
        "cursor_time": "2026-04-06T12:34:56.789Z",
        "cursor_age_secs": 2,
        "events": {
          "total": 12470,
          "indexed": 14,
          "ignored": 12456,
          "last_event_age_ms": 320
        },
        "per_collection": {
          "app.bsky.feed.post": 11200,
          "at.opake.document": 4
        }
      }
  """

  use OpakeIndexerWeb, :controller

  alias OpakeIndexer.Firehose.State
  alias OpakeIndexer.Queries.CursorQueries

  @micros_per_second 1_000_000

  def index(conn, _params) do
    snapshot = State.snapshot()
    cursor = CursorQueries.load_cursor()

    response = %{
      indexer_connected: snapshot.connected,
      cursor_time: format_cursor_time(cursor),
      cursor_age_secs: cursor_age_secs(cursor),
      events: %{
        total: snapshot.total,
        indexed: snapshot.indexed,
        ignored: snapshot.ignored,
        last_event_age_ms: snapshot.last_event_age_ms
      },
      per_collection: snapshot.per_collection
    }

    json(conn, response)
  end

  defp format_cursor_time(nil), do: nil

  defp format_cursor_time(%{time_us: time_us}) do
    time_us
    |> DateTime.from_unix!(:microsecond)
    |> DateTime.to_iso8601()
  end

  defp cursor_age_secs(nil), do: nil

  defp cursor_age_secs(%{time_us: time_us}) do
    now_us = DateTime.utc_now() |> DateTime.to_unix(:microsecond)
    div(now_us - time_us, @micros_per_second)
  end
end
