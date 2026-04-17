defmodule OpakeIndexerWeb.PaginationHelpers do
  @moduledoc """
  Shared parameter parsing for paginated API endpoints.
  """

  @default_limit 50
  @max_limit 100

  def parse_limit(%{"limit" => limit_str}) when is_binary(limit_str) do
    case Integer.parse(limit_str) do
      {n, ""} when n >= 1 and n <= @max_limit -> {:ok, n}
      {_, ""} -> {:error, "limit must be between 1 and #{@max_limit}"}
      _ -> {:error, "invalid limit"}
    end
  end

  def parse_limit(_), do: {:ok, @default_limit}

  def maybe_put_cursor(response, nil), do: response
  def maybe_put_cursor(response, cursor), do: Map.put(response, :cursor, cursor)
end
