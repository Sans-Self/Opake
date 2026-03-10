defmodule OpakeAppview.Queries.Pagination do
  @moduledoc """
  Shared cursor-based pagination helpers for query modules.

  Cursor format: `"{iso8601_indexed_at}::{uri}"`. The cursor encodes the
  position of the last item returned, enabling keyset pagination without offsets.
  """

  @doc """
  Parses a cursor string into `{:ok, datetime, uri}` or `:none`.
  """
  def parse_cursor(nil), do: :none
  def parse_cursor(""), do: :none

  def parse_cursor(cursor) do
    case String.split(cursor, "::", parts: 2) do
      [time_str, uri] ->
        case DateTime.from_iso8601(time_str) do
          {:ok, datetime, _offset} -> {:ok, datetime, uri}
          _ -> :none
        end

      _ ->
        :none
    end
  end

  @doc """
  Builds the next cursor from a list of results. Returns `nil` for empty lists.
  Items must have `:indexed_at` (DateTime) and `:uri` (string) fields.
  """
  def build_next_cursor([]), do: nil

  def build_next_cursor(items) do
    last = List.last(items)
    "#{DateTime.to_iso8601(last.indexed_at)}::#{last.uri}"
  end
end
