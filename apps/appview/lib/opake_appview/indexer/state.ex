defmodule OpakeAppview.Indexer.State do
  @moduledoc """
  Shared mutable state for the indexer pipeline.

  Lives in a single ETS table (`:indexer_state`) so the consumer process,
  the heartbeat process, and the health endpoint can all read it without
  coordinating through GenServer messages. The consumer is the sole
  writer; everyone else only reads.

  ## Layout

  Each row is a `{key, value}` pair:

    * `{:connected, boolean}` — current WebSocket connection state
    * `{:counter_total, integer}` — lifetime events received
    * `{:counter_ignored, integer}` — events the parser returned `:ignore` for
    * `{:counter_indexed, integer}` — events that hit a dispatch clause
    * `{:last_event_at_ms, integer}` — `System.monotonic_time(:millisecond)`
      of the last received frame, or `nil` if none yet
    * `{:cursor_time_us, integer | nil}` — last cursor we wrote to Postgres
    * `{:cursor_saved_at_ms, integer | nil}` — monotonic ms of the last save
    * `{{:collection, name}, integer}` — per-collection event count
      (key includes both indexed and ignored — distinguished by name:
      opake.* are indexed, everything else is ignored)

  ## Atomicity

  Counters use `:ets.update_counter/3` for atomic increments. Other writes
  use `:ets.insert/2` and are inherently atomic for single rows. Heartbeat
  reads may briefly observe partial updates between counters; that's fine
  because the displayed numbers are advisory, not load-bearing.
  """

  @table :indexer_state

  @counters [:counter_total, :counter_ignored, :counter_indexed]

  @spec init() :: :ets.tid() | atom()
  def init do
    table =
      :ets.new(@table, [
        :named_table,
        :set,
        :public,
        read_concurrency: true,
        write_concurrency: true
      ])

    :ets.insert(@table, {:connected, false})
    Enum.each(@counters, &:ets.insert(@table, {&1, 0}))
    :ets.insert(@table, {:last_event_at_ms, nil})
    :ets.insert(@table, {:cursor_time_us, nil})
    :ets.insert(@table, {:cursor_saved_at_ms, nil})

    table
  end

  # -- Connection --

  @spec set_connected(boolean()) :: true
  def set_connected(connected) when is_boolean(connected) do
    :ets.insert(@table, {:connected, connected})
  end

  @spec connected?() :: boolean()
  def connected? do
    case lookup(:connected) do
      val when is_boolean(val) -> val
      _ -> false
    end
  end

  # -- Counters --

  @spec bump_total() :: integer()
  def bump_total, do: :ets.update_counter(@table, :counter_total, 1, {:counter_total, 0})

  @spec bump_ignored() :: integer()
  def bump_ignored, do: :ets.update_counter(@table, :counter_ignored, 1, {:counter_ignored, 0})

  @spec bump_indexed() :: integer()
  def bump_indexed, do: :ets.update_counter(@table, :counter_indexed, 1, {:counter_indexed, 0})

  @spec bump_collection(String.t() | nil) :: integer()
  def bump_collection(nil), do: 0

  def bump_collection(name) when is_binary(name) do
    :ets.update_counter(@table, {:collection, name}, 1, {{:collection, name}, 0})
  end

  @spec counter(atom()) :: integer()
  def counter(key) when key in @counters do
    case lookup(key) do
      n when is_integer(n) -> n
      _ -> 0
    end
  end

  @spec collection_counts() :: %{String.t() => integer()}
  def collection_counts do
    @table
    |> :ets.match({{:collection, :"$1"}, :"$2"})
    |> Map.new(fn [name, count] -> {name, count} end)
  end

  # -- Last event --

  @spec mark_event_received() :: true
  def mark_event_received do
    :ets.insert(@table, {:last_event_at_ms, System.monotonic_time(:millisecond)})
  end

  @spec last_event_age_ms() :: non_neg_integer() | nil
  def last_event_age_ms do
    case lookup(:last_event_at_ms) do
      ms when is_integer(ms) -> System.monotonic_time(:millisecond) - ms
      _ -> nil
    end
  end

  # -- Cursor --

  @spec record_cursor_save(integer()) :: true
  def record_cursor_save(time_us) when is_integer(time_us) do
    :ets.insert(@table, {:cursor_time_us, time_us})
    :ets.insert(@table, {:cursor_saved_at_ms, System.monotonic_time(:millisecond)})
  end

  @spec cursor_saved_age_ms() :: non_neg_integer() | nil
  def cursor_saved_age_ms do
    case lookup(:cursor_saved_at_ms) do
      ms when is_integer(ms) -> System.monotonic_time(:millisecond) - ms
      _ -> nil
    end
  end

  @spec last_cursor_time_us() :: integer() | nil
  def last_cursor_time_us, do: lookup(:cursor_time_us)

  # -- Snapshot for heartbeat / health endpoint --

  @type snapshot :: %{
          connected: boolean(),
          total: integer(),
          ignored: integer(),
          indexed: integer(),
          last_event_age_ms: non_neg_integer() | nil,
          cursor_time_us: integer() | nil,
          cursor_saved_age_ms: non_neg_integer() | nil,
          per_collection: %{String.t() => integer()}
        }

  @spec snapshot() :: snapshot()
  def snapshot do
    %{
      connected: connected?(),
      total: counter(:counter_total),
      ignored: counter(:counter_ignored),
      indexed: counter(:counter_indexed),
      last_event_age_ms: last_event_age_ms(),
      cursor_time_us: last_cursor_time_us(),
      cursor_saved_age_ms: cursor_saved_age_ms(),
      per_collection: collection_counts()
    }
  end

  # -- Internal --

  defp lookup(key) do
    case :ets.lookup(@table, key) do
      [{^key, val}] -> val
      [] -> nil
    end
  end
end
