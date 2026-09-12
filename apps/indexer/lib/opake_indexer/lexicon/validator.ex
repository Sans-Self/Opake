defmodule OpakeIndexer.Lexicon.Validator do
  @moduledoc """
  The indexer ingest gate (D6): the network-hygiene layer that refuses
  malformed and vocabulary-violating `at.opake.*` records before they enter the
  `records` table, snapshots, or streams.

  Refusal is **rejection, not deletion** — the record stays on the author's PDS;
  the indexer simply declines to index or relay it, mirroring the authority
  rejection precedent (`OpakeIndexer.Authority`). This is untrusted network
  hygiene, not a trust anchor: client-side read lenience stands regardless of
  whether this gate ran, because pre-gate history may already carry poison and
  an author controls their own PDS.

  Two-step gate, matching `record-validity`:

    1. **Structural validation for all versions.** Every incoming record is
       validated against the newest shipped lexicon (`Schema`). Additive
       evolution guarantees a well-formed record of any version — including one
       declaring a version above the indexer's known range — passes this floor.
       A claimed future version is therefore NOT a structural bypass: garbage
       stamped `opakeVersion: 999` still fails the structural check and is
       refused like any other malformed record.

    2. **Vocabulary enforcement for known versions only.** For a record whose
       declared version is within the indexer's known range
       (`Vocabulary.max_version/0`), every registry value it carries must be in
       that version's cumulative vocabulary. Records declaring a future version
       skip this check — there is no lock-step federation; a well-formed
       future-version record is indexed and relayed verbatim.

  The vocabulary-bearing value sites are the closed, enumerated set from D8
  (key-wrap algo identifiers, content-encryption algo identifiers, keyring
  member roles) as they appear in the four collections the indexer ingests.
  Extending that set is itself a schema-version bump. Structural discriminants
  (union `$type`) are not vocabulary and are not checked here.
  """

  require Logger

  alias OpakeIndexer.Lexicon.{Schema, Vocabulary}

  @type result :: :ok | {:refused, term()}

  @known_max_version Vocabulary.max_version()

  @key_wrap_algo "keyWrapAlgo"
  @content_encryption_algo "contentEncryptionAlgo"
  @public_key_algo "publicKeyAlgo"
  @public_key_signature_algo "publicKeySignatureAlgo"
  @keyring_member_role "keyringMemberRole"

  @doc """
  Gate one incoming record. Returns `:ok` to index it, or `{:refused, reason}`
  to drop it (no index, no broadcast). `reason` is `{:malformed, detail}` for a
  structural failure or `{:vocabulary, {field, value}}` for a known-version
  vocabulary violation.
  """
  @spec validate(String.t(), term()) :: result()
  def validate(collection, record) do
    case Schema.validate(collection, record) do
      :ok ->
        with :ok <- validate_collection_semantics(collection, record) do
          vocabulary_gate(collection, record)
        end

      {:error, reason} ->
        {:refused, {:malformed, reason}}
    end
  end

  # Structural validation guarantees `opakeVersion` is a present integer here.
  defp vocabulary_gate(collection, record) do
    version = Map.fetch!(record, "opakeVersion")

    if version <= @known_max_version do
      check_vocabulary(collection, record, version)
    else
      # Future version: structural check only, relayed verbatim.
      :ok
    end
  end

  defp check_vocabulary(collection, record, version) do
    collection
    |> vocabulary_pairs(record)
    |> Enum.reduce_while(:ok, fn {field, value}, :ok ->
      if Vocabulary.permits?(field, version, value) do
        {:cont, :ok}
      else
        {:halt, {:refused, {:vocabulary, {field, value}}}}
      end
    end)
  end

  # -- Vocabulary extraction (the closed D8 field set, per collection) --
  #
  # Defensive throughout: structural validation has already run, so shapes are
  # sound, but a union may be the other variant and optional arrays may be
  # absent. Only string values that are actually present are yielded.

  defp vocabulary_pairs("at.opake.grant", record) do
    wrapped_key_algo(record["wrappedKey"])
  end

  defp vocabulary_pairs("at.opake.keyring", record) do
    content = string_pair(@content_encryption_algo, record["algo"])
    members = member_pairs(record["members"])

    history =
      record
      |> Map.get("keyHistory", [])
      |> List.wrap()
      |> Enum.flat_map(fn entry -> member_pairs(entry["members"]) end)

    content ++ members ++ history
  end

  defp vocabulary_pairs("at.opake.publicKey", record) do
    string_pair(@public_key_algo, record["x25519Algo"]) ++
      string_pair(@public_key_algo, record["mlKemAlgo"]) ++
      string_pair(@public_key_algo, record["signingAlgo"]) ++
      string_pair(@public_key_signature_algo, record["signatureAlgo"])
  end

  defp vocabulary_pairs("at.opake.directory", record) do
    key_wrapping_pairs(record["keyWrapping"])
  end

  defp vocabulary_pairs("at.opake.document", record) do
    encryption_pairs(record["encryption"])
  end

  defp vocabulary_pairs(_collection, _record), do: []

  # keyWrapping union — only the direct-wrapping variant carries algo identifiers.
  defp key_wrapping_pairs(%{"$type" => "at.opake.defs#directKeyWrapping", "keys" => keys})
       when is_list(keys),
       do: Enum.flat_map(keys, &wrapped_key_algo/1)

  defp key_wrapping_pairs(_), do: []

  # encryption union — direct carries a content algo plus per-key wrap algos;
  # keyring carries a content algo.
  defp encryption_pairs(%{
         "$type" => "at.opake.document#directEncryption",
         "envelope" => envelope
       })
       when is_map(envelope) do
    algo = string_pair(@content_encryption_algo, envelope["algo"])

    keys =
      case envelope["keys"] do
        keys when is_list(keys) -> Enum.flat_map(keys, &wrapped_key_algo/1)
        _ -> []
      end

    algo ++ keys
  end

  defp encryption_pairs(%{"$type" => "at.opake.document#keyringEncryption", "algo" => algo}),
    do: string_pair(@content_encryption_algo, algo)

  defp encryption_pairs(_), do: []

  defp member_pairs(members) when is_list(members) do
    Enum.flat_map(members, fn member ->
      string_pair(@keyring_member_role, member["role"]) ++ wrapped_key_algo(member["wrappedKey"])
    end)
  end

  defp member_pairs(_), do: []

  defp wrapped_key_algo(%{"algo" => algo}) when is_binary(algo),
    do: [{@key_wrap_algo, algo}]

  defp wrapped_key_algo(_), do: []

  defp string_pair(field, value) when is_binary(value), do: [{field, value}]
  defp string_pair(_field, _value), do: []

  # The lexicon walker establishes shape. These rules bind the fields inside a
  # member and reject duplicate DIDs in every current/history snapshot.
  defp validate_collection_semantics("at.opake.keyring", record) do
    with :ok <- validate_member_list(record["members"] || []) do
      record
      |> Map.get("keyHistory", [])
      |> List.wrap()
      |> Enum.reduce_while(:ok, fn entry, :ok ->
        case validate_member_list(entry["members"] || []) do
          :ok -> {:cont, :ok}
          {:refused, _} = refusal -> {:halt, refusal}
        end
      end)
    end
  end

  defp validate_collection_semantics(_collection, _record), do: :ok

  defp validate_member_list(members) when is_list(members) do
    Enum.reduce_while(members, {:ok, MapSet.new()}, fn member, {:ok, dids} ->
      case validate_member(member, dids) do
        {:ok, did} -> {:cont, {:ok, MapSet.put(dids, did)}}
        {:refused, _} = refusal -> {:halt, refusal}
      end
    end)
    |> case do
      {:ok, _dids} -> :ok
      {:refused, _} = refusal -> refusal
    end
  end

  defp validate_member(%{"did" => did, "role" => _role} = member, dids)
       when is_binary(did) do
    cond do
      not String.starts_with?(did, "did:") ->
        {:refused, {:malformed, :member_did}}

      MapSet.member?(dids, did) ->
        {:refused, {:malformed, :duplicate_member_did}}

      match?(%{"did" => wrap_did} when wrap_did != did, member["wrappedKey"]) ->
        {:refused, {:malformed, :member_wrap_did_mismatch}}

      true ->
        {:ok, did}
    end
  end

  defp validate_member(_member, _dids), do: {:refused, {:malformed, :member_shape}}
end
