defmodule OpakeAppview.Queries.CursorQueries do
  @moduledoc """
  Read/write the singleton Jetstream cursor. The cursor tracks how far
  the indexer has consumed the firehose, enabling resumption after restarts.
  """

  alias OpakeAppview.Repo
  alias OpakeAppview.Schemas.Cursor

  @micros_per_second 1_000_000

  def load_cursor do
    Repo.get(Cursor, 1)
  end

  def save_cursor(time_us) do
    now = DateTime.utc_now()

    %Cursor{id: 1}
    |> Cursor.changeset(%{id: 1, time_us: time_us, updated_at: now})
    |> Repo.insert(
      on_conflict: [set: [time_us: time_us, updated_at: now]],
      conflict_target: :id
    )
  end

  def cursor_age_secs do
    case load_cursor() do
      nil ->
        nil

      %Cursor{time_us: time_us} ->
        now_us = DateTime.utc_now() |> DateTime.to_unix(:microsecond)
        div(now_us - time_us, @micros_per_second)
    end
  end
end
