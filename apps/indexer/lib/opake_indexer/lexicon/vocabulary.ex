defmodule OpakeIndexer.Lexicon.Vocabulary do
  @moduledoc """
  Version-pinned registry vocabulary for `at.opake.*` records.

  Reads `lexicons/vocabulary.json` — the language-neutral single source of
  truth shared with the Rust core (see `openspec/specs/record-validity`, D8).
  The core binds the same artifact via `include_str!`; the two consumers are
  kept honest by a cross-language conformance test.

  The artifact is embedded at compile time through `@external_resource`, so a
  change to `vocabulary.json` recompiles this module and there is no runtime
  path to lose in a release. This is the BEAM equivalent of the core's
  compile-time bind: the same bytes, no hand-copied table, drift impossible.

  Vocabulary is **cumulative**: a value pinned at version N is permitted at
  every version ≥ N. A record declaring a known version N that carries a
  registry value outside version N's cumulative set is corrupt on vocabulary
  grounds. Structural discriminants (union `$type` tags) are deliberately
  absent — a new union variant is a structural change and takes the new-NSID
  path, never a vocabulary bump.
  """

  @vocab_path Path.expand(Path.join(__DIR__, "../../../../../lexicons/vocabulary.json"))
  @external_resource @vocab_path

  @raw @vocab_path |> File.read!() |> Jason.decode!()

  # field_key => %{version_integer => [values pinned AT that version]}.
  # Non-integer keys ($comment) are metadata and skipped.
  @by_field (for {field, per_version} <- Map.fetch!(@raw, "fields"), into: %{} do
               versions =
                 for {version_key, values} <- per_version,
                     match?({_int, ""}, Integer.parse(version_key)),
                     into: %{} do
                   {version, ""} = Integer.parse(version_key)
                   {version, values}
                 end

               {field, versions}
             end)

  @field_keys Map.keys(@by_field)

  @max_version @by_field
               |> Map.values()
               |> Enum.flat_map(&Map.keys/1)
               |> Enum.max(fn -> 0 end)

  @doc "The closed list of vocabulary-bearing field keys, as declared in the artifact."
  @spec field_keys() :: [String.t()]
  def field_keys, do: @field_keys

  @doc "The highest schema version the indexer knows vocabulary for (its known range ceiling)."
  @spec max_version() :: non_neg_integer()
  def max_version, do: @max_version

  @doc "Version keys explicitly declared for `field` (ascending)."
  @spec versions(String.t()) :: [pos_integer()]
  def versions(field), do: @by_field |> Map.get(field, %{}) |> Map.keys() |> Enum.sort()

  @doc "Values pinned AT exactly `version` for `field` (non-cumulative — for conformance checks)."
  @spec values_at(String.t(), pos_integer()) :: [String.t()]
  def values_at(field, version), do: @by_field |> Map.get(field, %{}) |> Map.get(version, [])

  @doc "The cumulative set of values permitted for `field` at `version` (every value pinned ≤ version)."
  @spec permitted(String.t(), non_neg_integer()) :: [String.t()]
  def permitted(field, version) do
    @by_field
    |> Map.get(field, %{})
    |> Enum.filter(fn {v, _values} -> v <= version end)
    |> Enum.flat_map(fn {_v, values} -> values end)
  end

  @doc """
  Whether `value` is a permitted vocabulary value for `field` under a record
  declaring schema `version`. Cumulative. A `false` result means the record is
  corrupt on vocabulary grounds.
  """
  @spec permits?(String.t(), non_neg_integer(), String.t()) :: boolean()
  def permits?(field, version, value) do
    @by_field
    |> Map.get(field, %{})
    |> Enum.any?(fn {v, values} -> v <= version and value in values end)
  end
end
