defmodule OpakeIndexer.Lexicon.Schema do
  @moduledoc """
  A minimal Lexicon structural validator over `at.opake.*` records.

  Driven entirely by the shipped `lexicons/*.json` schemas (embedded at compile
  time via `@external_resource`, same posture as `Vocabulary`). There is no
  mature Elixir Lexicon validator and none is warranted here — this covers only
  the `at.opake.*` collections, checking the two things the ingest gate needs:

    * every **required** field of the newest shipped schema is present, and
    * every **known** field that IS present carries a value of its declared
      type (recursively through refs, unions, arrays and nested objects).

  Crucially it is **lenient about unknown fields**: additive schema evolution
  (D8) guarantees a well-formed record of any future version still carries every
  past version's required fields, but may also carry fields this indexer has
  never heard of. Rejecting those would turn routine version skew into a false
  corruption. So unknown keys pass; only missing-required and wrong-typed-known
  fail. This is what lets a well-formed future-version record clear structural
  validation and be relayed verbatim.

  Byte scalars (`bytes` → `{"$bytes": ...}`) are base64-decoded and checked
  against their lexicon bounds. Other opaque AT Protocol scalars (`cid-link` →
  `{"$link": ...}`, `blob`, `unknown`) are presence-checked only.

  Union variants are validated strictly: a `$type` outside the schema's declared
  refs is a structural failure, because a new union variant is a new-NSID change
  (D8), never something a higher `opakeVersion` within the same collection may
  introduce.
  """

  @lexicon_dir Path.expand(Path.join(__DIR__, "../../../../../lexicons"))
  @lexicon_files Path.wildcard(Path.join(@lexicon_dir, "at.opake.*.json"))

  for file <- @lexicon_files do
    @external_resource file
  end

  # NSID => decoded lexicon document. Includes `at.opake.defs` (ref target) and
  # non-record docs (e.g. permission sets); only collections with a
  # `defs.main.record` object are structurally validatable.
  @lexicons (for file <- @lexicon_files, into: %{} do
               decoded = file |> File.read!() |> Jason.decode!()
               {decoded["id"], decoded}
             end)

  @type reason :: term()

  @doc "The loaded lexicon documents, keyed by NSID. Introspection for tests."
  @spec lexicons() :: %{optional(String.t()) => map()}
  def lexicons, do: @lexicons

  @doc """
  Structurally validate `record` against the shipped schema for `collection`.

  Returns `:ok` or `{:error, reason}`. Unknown collections (no shipped record
  schema) are an error — the ingest gate only routes the four indexed
  collections here, so an unknown collection means a wiring bug, not a
  legitimate record.
  """
  @spec validate(String.t(), term()) :: :ok | {:error, reason()}
  def validate(collection, record) when is_map(record) do
    with %{} = lexicon <-
           Map.get(@lexicons, collection, {:error, {:unknown_collection, collection}}),
         %{} = schema <- get_in(lexicon, ["defs", "main", "record"]) do
      validate_type(schema, record, collection)
    else
      {:error, _} = err -> err
      nil -> {:error, {:no_record_def, collection}}
    end
  end

  def validate(_collection, _record), do: {:error, :not_an_object}

  # -- Type dispatch --------------------------------------------------

  # A required field that is absent or explicitly null is reported as missing
  # (both read as "not there" for structural purposes), so nil never reaches
  # validate_type from the object walk. A nil arriving any other way is invalid.
  defp validate_type(_schema, nil, _nsid), do: {:error, :null_value}

  defp validate_type(%{"type" => "object"} = schema, value, nsid),
    do: validate_object(schema, value, nsid)

  defp validate_type(%{"type" => "ref", "ref" => ref}, value, nsid) do
    {_full, target_nsid, name} = normalize_ref(ref, nsid)

    case resolve_def(target_nsid, name) do
      nil -> {:error, {:unresolved_ref, ref}}
      def_schema -> validate_type(def_schema, value, target_nsid)
    end
  end

  defp validate_type(%{"type" => "union", "refs" => refs}, value, nsid)
       when is_map(value) do
    type = Map.get(value, "$type")
    variants = Enum.map(refs, &normalize_ref(&1, nsid))

    case Enum.find(variants, fn {full, _nsid, _name} -> full == type end) do
      nil ->
        {:error, {:unknown_union_variant, type}}

      {_full, target_nsid, name} ->
        case resolve_def(target_nsid, name) do
          nil -> {:error, {:unresolved_ref, name}}
          def_schema -> validate_type(def_schema, value, target_nsid)
        end
    end
  end

  defp validate_type(%{"type" => "union"}, _value, _nsid), do: {:error, :union_not_object}

  defp validate_type(%{"type" => "array", "items" => items}, value, nsid) when is_list(value) do
    Enum.reduce_while(value, :ok, fn element, :ok ->
      case validate_type(items, element, nsid) do
        :ok -> {:cont, :ok}
        {:error, _} = err -> {:halt, err}
      end
    end)
  end

  defp validate_type(%{"type" => "array"}, value, _nsid) when is_list(value), do: :ok
  defp validate_type(%{"type" => "array"}, _value, _nsid), do: {:error, :not_an_array}

  defp validate_type(%{"type" => "integer"} = schema, value, _nsid) do
    cond do
      not is_integer(value) ->
        {:error, {:not_an_integer, value}}

      Map.has_key?(schema, "minimum") and value < schema["minimum"] ->
        {:error, {:below_minimum, value}}

      true ->
        :ok
    end
  end

  defp validate_type(%{"type" => "string"}, value, _nsid) when is_binary(value), do: :ok
  defp validate_type(%{"type" => "string"}, value, _nsid), do: {:error, {:not_a_string, value}}

  defp validate_type(%{"type" => "boolean"}, value, _nsid) when is_boolean(value), do: :ok
  defp validate_type(%{"type" => "boolean"}, value, _nsid), do: {:error, {:not_a_boolean, value}}

  defp validate_type(%{"type" => "number"}, value, _nsid) when is_number(value), do: :ok
  defp validate_type(%{"type" => "number"}, value, _nsid), do: {:error, {:not_a_number, value}}

  defp validate_type(%{"type" => "bytes"} = schema, %{"$bytes" => encoded}, _nsid)
       when is_binary(encoded) do
    with {:ok, bytes} <- OpakeIndexer.Auth.Base64.decode(encoded),
         :ok <- check_byte_bounds(schema, byte_size(bytes)) do
      :ok
    end
  end

  defp validate_type(%{"type" => "bytes"}, _value, _nsid), do: {:error, :not_atproto_bytes}

  # Opaque atproto-encoded scalars: presence already established by the object
  # walk (non-nil), so any non-nil value passes. See moduledoc.
  defp validate_type(%{"type" => opaque}, _value, _nsid)
       when opaque in ["cid-link", "blob", "unknown"],
       do: :ok

  # Unknown/absent type marker — treat as opaque rather than erroring, so a
  # schema construct this minimal validator doesn't model can't wedge ingest.
  defp validate_type(_schema, _value, _nsid), do: :ok

  # -- Object validation ----------------------------------------------

  defp validate_object(schema, value, nsid) when is_map(value) do
    required = Map.get(schema, "required", [])
    properties = Map.get(schema, "properties", %{})

    with :ok <- check_required(required, value) do
      validate_present_properties(properties, value, nsid)
    end
  end

  defp validate_object(_schema, _value, _nsid), do: {:error, :not_an_object}

  defp check_byte_bounds(schema, size) do
    cond do
      Map.has_key?(schema, "minLength") and size < schema["minLength"] ->
        {:error, {:below_minimum_byte_length, size}}

      Map.has_key?(schema, "maxLength") and size > schema["maxLength"] ->
        {:error, {:above_maximum_byte_length, size}}

      true ->
        :ok
    end
  end

  defp check_required(required, value) do
    case Enum.find(required, fn field -> is_nil(Map.get(value, field)) end) do
      nil -> :ok
      field -> {:error, {:missing_required, field}}
    end
  end

  # Validate only the fields the schema knows AND the record actually carries
  # (non-nil). Unknown keys are ignored — the additive-evolution leniency that
  # lets future-version records through.
  defp validate_present_properties(properties, value, nsid) do
    Enum.reduce_while(properties, :ok, fn {field, field_schema}, :ok ->
      case Map.get(value, field) do
        nil ->
          {:cont, :ok}

        field_value ->
          case validate_type(field_schema, field_value, nsid) do
            :ok -> {:cont, :ok}
            {:error, reason} -> {:halt, {:error, {field, reason}}}
          end
      end
    end)
  end

  # -- Ref resolution -------------------------------------------------

  # `"at.opake.defs#name"` → {full, "at.opake.defs", "name"};
  # `"#name"` (local) → {"<nsid>#name", nsid, "name"}. The full form matches the
  # `$type` discriminant a union value carries.
  defp normalize_ref(ref, current_nsid) do
    case String.split(ref, "#", parts: 2) do
      ["", name] -> {current_nsid <> "#" <> name, current_nsid, name}
      [nsid, name] -> {ref, nsid, name}
      [name] -> {current_nsid <> "#" <> name, current_nsid, name}
    end
  end

  defp resolve_def(nsid, name), do: get_in(@lexicons, [nsid, "defs", name])
end
