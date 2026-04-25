defmodule OpakeIndexer.Queries.CursorQueries do
  @moduledoc """
  Read/write the singleton Jetstream cursor. The cursor tracks how far
  the indexer has consumed the firehose, enabling resumption after restarts.
  """

  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Cursor

  @spec load_cursor() :: Cursor.t() | nil
  def load_cursor do
    Repo.get(Cursor, 1)
  end

  @spec save_cursor(integer()) :: {:ok, Cursor.t()} | {:error, Ecto.Changeset.t()}
  def save_cursor(time_us) do
    now = DateTime.utc_now()

    %Cursor{id: 1}
    |> Cursor.changeset(%{id: 1, time_us: time_us, updated_at: now})
    |> Repo.insert(
      on_conflict: [set: [time_us: time_us, updated_at: now]],
      conflict_target: :id
    )
  end
end
