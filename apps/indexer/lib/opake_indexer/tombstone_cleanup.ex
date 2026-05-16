defmodule OpakeIndexer.TombstoneCleanup do
  @moduledoc """
  Periodic cleanup of soft-deleted records. Runs every hour, purges
  tombstones older than 7 days from the unified `records` table.
  """

  use GenServer
  require Logger

  import Ecto.Query
  alias OpakeIndexer.Repo
  alias OpakeIndexer.Schemas.Record, as: RecordSchema

  @cleanup_interval :timer.hours(1)
  @tombstone_ttl_days 7

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(_opts) do
    GenServer.start_link(__MODULE__, :ok, name: __MODULE__)
  end

  @impl true
  def init(:ok) do
    schedule_cleanup()
    {:ok, %{}}
  end

  @impl true
  def handle_info(:cleanup, state) do
    cutoff = DateTime.add(DateTime.utc_now(), -@tombstone_ttl_days * 24 * 3600, :second)

    {purged, _} =
      from(r in RecordSchema, where: not is_nil(r.deleted_at) and r.deleted_at < ^cutoff)
      |> Repo.delete_all()

    if purged > 0 do
      Logger.info("Tombstone cleanup: purged #{purged} records")
    end

    schedule_cleanup()
    {:noreply, state}
  end

  defp schedule_cleanup do
    Process.send_after(self(), :cleanup, @cleanup_interval)
  end
end
