defmodule OpakeAppview.SSE.ConnectionTracker do
  @moduledoc """
  Tracks active SSE connections per DID to prevent runaway reconnection
  loops from exhausting BEAM processes. Simple ETS counter.
  """

  @table :sse_connections
  @max_per_did 5

  @spec init_table() :: :ok
  def init_table do
    :ets.new(@table, [:named_table, :set, :public, write_concurrency: true])
    :ok
  end

  @spec acquire(String.t()) :: :ok | {:error, :limit_reached}
  def acquire(did) do
    count = :ets.update_counter(@table, did, {2, 1}, {did, 0})

    if count > @max_per_did do
      :ets.update_counter(@table, did, {2, -1})
      {:error, :limit_reached}
    else
      :ok
    end
  end

  @spec release(String.t()) :: :ok
  def release(did) do
    :ets.update_counter(@table, did, {2, -1, 0, 0}, {did, 0})
    :ok
  end
end
