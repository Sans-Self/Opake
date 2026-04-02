defmodule OpakeAppview.Schemas.Cursor do
  @moduledoc """
  Singleton row tracking the Jetstream cursor position. The `id = 1` CHECK
  constraint enforces exactly one row. `time_us` is the Jetstream event
  timestamp in Unix microseconds.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :integer, autogenerate: false}
  schema "cursor" do
    field :time_us, :integer
    field :updated_at, :utc_datetime_usec
  end

  def changeset(cursor, attrs) do
    cursor
    |> cast(attrs, [:id, :time_us, :updated_at])
    |> validate_required([:id, :time_us, :updated_at])
    |> validate_inclusion(:id, [1])
  end
end
