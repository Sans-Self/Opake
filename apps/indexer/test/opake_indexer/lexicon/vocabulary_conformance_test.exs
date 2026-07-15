defmodule OpakeIndexer.Lexicon.VocabularyConformanceTest do
  @moduledoc """
  Cross-language conformance: the Elixir `Vocabulary` loader MUST read the exact
  field / version / value sets that `lexicons/vocabulary.json` declares — the
  same artifact the Rust core binds via `include_str!`.

  This is the drift guard called out in `record-validity` D8: hand-copying the
  table into either language's constants is the failure mode, and this test
  catches it by comparing the loader against a fresh independent read of the
  JSON. If Rust and Elixir both conform to the artifact, they cannot diverge
  from each other.

  The pinned v1 assertions mirror the Rust `vocabulary::tests` value checks, so
  a change on either side that isn't reflected in the shared JSON fails here.
  """

  use ExUnit.Case, async: true

  alias OpakeIndexer.Lexicon.Vocabulary

  # Independent read of the artifact — deliberately NOT going through the loader,
  # so this is a genuine second opinion on the same bytes.
  @artifact_path Path.expand(Path.join(__DIR__, "../../../../../lexicons/vocabulary.json"))
  @raw_fields @artifact_path |> File.read!() |> Jason.decode!() |> Map.fetch!("fields")

  defp declared_versions(field_map) do
    for {key, values} <- field_map,
        match?({_int, ""}, Integer.parse(key)),
        do: {elem(Integer.parse(key), 0), values}
  end

  describe "loader conforms to the artifact" do
    test "reads exactly the field keys the JSON declares" do
      assert Enum.sort(Vocabulary.field_keys()) == Enum.sort(Map.keys(@raw_fields))
    end

    test "reads exactly the declared versions and per-version value sets for every field" do
      for {field, field_map} <- @raw_fields do
        declared = declared_versions(field_map)

        assert Enum.sort(Vocabulary.versions(field)) ==
                 declared |> Enum.map(&elem(&1, 0)) |> Enum.sort(),
               "version keys drift for #{field}"

        for {version, values} <- declared do
          assert Enum.sort(Vocabulary.values_at(field, version)) == Enum.sort(values),
                 "value set drift for #{field} v#{version}"
        end
      end
    end

    test "max_version equals the highest declared version across all fields" do
      highest =
        @raw_fields
        |> Enum.flat_map(fn {_field, field_map} ->
          declared_versions(field_map) |> Enum.map(&elem(&1, 0))
        end)
        |> Enum.max()

      assert Vocabulary.max_version() == highest
    end
  end

  describe "cumulative semantics match the Rust interpretation" do
    # These mirror crates/opake-core/src/records/vocabulary.rs tests.
    test "v1 values are permitted at v1" do
      assert Vocabulary.permits?("keyWrapAlgo", 1, "x25519-mlkem768-hkdf-a256kw-v2")
      assert Vocabulary.permits?("contentEncryptionAlgo", 1, "aes-256-gcm")
      assert Vocabulary.permits?("publicKeyAlgo", 1, "ml-kem-768")
      assert Vocabulary.permits?("pairingAlgo", 1, "x25519-mlkem768")
      assert Vocabulary.permits?("keyringMemberRole", 1, "manager")
    end

    test "unknown values are not permitted" do
      refute Vocabulary.permits?("keyWrapAlgo", 1, "rot13")
      refute Vocabulary.permits?("keyringMemberRole", 1, "superuser")
    end

    test "vocabulary is cumulative — v1 values stay valid at higher versions" do
      assert Vocabulary.permits?("keyWrapAlgo", 6, "x25519-mlkem768-hkdf-a256kw-v2")
    end

    test "a value is not permitted below the version that pins it" do
      # aes-256-gcm is pinned at v1, so it is not permitted at v0.
      refute Vocabulary.permits?("contentEncryptionAlgo", 0, "aes-256-gcm")
    end
  end
end
