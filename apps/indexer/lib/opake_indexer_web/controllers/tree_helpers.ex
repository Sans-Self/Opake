defmodule OpakeIndexerWeb.TreeHelpers do
  @moduledoc """
  Shared helpers for record-returning controllers.

  Every record response is wrapped in the envelope:

      %{
        record: <verbatim PDS JSON>,
        indexedAt: ISO8601,
        deletedAt: ISO8601 | nil
      }

  No field-flattening, no per-collection format helpers. The `record`
  field is byte-identical to what the PDS holds; indexer metadata lives
  alongside it as siblings.
  """

  alias OpakeIndexer.Schemas.Record, as: RecordSchema

  @spec envelope(RecordSchema.t()) :: map()
  def envelope(%RecordSchema{
        uri: uri,
        record_jsonb: jsonb,
        indexed_at: indexed_at,
        deleted_at: deleted_at
      }) do
    base = %{
      uri: uri,
      record: jsonb,
      indexedAt: format_datetime(indexed_at)
    }

    case deleted_at do
      nil -> base
      ts -> Map.put(base, :deletedAt, format_datetime(ts))
    end
  end

  @spec format_tree_response([RecordSchema.t()], [RecordSchema.t()], DateTime.t()) :: map()
  def format_tree_response(directories, documents, server_time) do
    %{
      directories: Enum.map(directories, &envelope/1),
      documents: Enum.map(documents, &envelope/1),
      server_time: DateTime.to_iso8601(server_time)
    }
  end

  @spec parse_since(map()) :: {:ok, DateTime.t()} | {:error, String.t()}
  def parse_since(%{"since" => since_str}) when is_binary(since_str) do
    case DateTime.from_iso8601(since_str) do
      {:ok, dt, _} -> {:ok, dt}
      {:error, _} -> {:error, "invalid since timestamp"}
    end
  end

  def parse_since(_), do: {:error, "since parameter is required"}

  defp format_datetime(nil), do: nil
  defp format_datetime(%DateTime{} = dt), do: DateTime.to_iso8601(dt)
end
