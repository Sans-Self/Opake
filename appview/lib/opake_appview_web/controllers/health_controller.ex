defmodule OpakeAppviewWeb.HealthController do
  @moduledoc """
  Unauthenticated health endpoint. Returns indexer connection state, cursor
  position, and cursor lag. Intentionally omits row counts — those are internal
  metrics, not public health signals.
  """

  use OpakeAppviewWeb, :controller

  alias OpakeAppview.Indexer
  alias OpakeAppview.Queries.CursorQueries

  @micros_per_second 1_000_000

  def index(conn, _params) do
    cursor = CursorQueries.load_cursor()

    response = %{
      indexerConnected: Indexer.connected?(),
      cursorTime: format_cursor_time(cursor),
      cursorAgeSecs: cursor_age_secs(cursor)
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
