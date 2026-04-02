defmodule OpakeAppview.TombstoneCleanup do
  @moduledoc """
  Periodic cleanup of soft-deleted directory and document records.
  Runs every hour, purges tombstones older than 7 days.
  """

  use GenServer
  require Logger

  alias OpakeAppview.Queries.DirectoryQueries

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
    {dirs, docs} = DirectoryQueries.purge_tombstones(cutoff)

    if dirs > 0 or docs > 0 do
      Logger.info("Tombstone cleanup: purged #{dirs} directories, #{docs} documents")
    end

    schedule_cleanup()
    {:noreply, state}
  end

  defp schedule_cleanup do
    Process.send_after(self(), :cleanup, @cleanup_interval)
  end
end
