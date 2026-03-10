defmodule OpakeAppview.Jetstream.Event do
  @moduledoc """
  Parses raw Jetstream JSON messages into tagged tuples for the indexer.

  Only `app.opake.grant` and `app.opake.keyring` commit events are recognized.
  Everything else (identity events, unknown collections, malformed JSON) returns
  `:ignore`. No full record validation — the appview indexes metadata fields,
  not crypto payloads.
  """

  @grant_collection "app.opake.grant"
  @keyring_collection "app.opake.keyring"

  def parse(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, payload} -> parse_payload(payload)
      {:error, _} -> :ignore
    end
  end

  defp parse_payload(%{"kind" => "commit", "did" => did, "time_us" => time_us, "commit" => commit}) do
    parse_commit(did, time_us, commit)
  end

  defp parse_payload(_), do: :ignore

  defp parse_commit(did, time_us, %{
         "operation" => operation,
         "collection" => collection,
         "rkey" => rkey
       } = commit) do
    uri = "at://#{did}/#{collection}/#{rkey}"

    case {collection, operation} do
      {@grant_collection, op} when op in ["create", "update"] ->
        parse_grant_upsert(uri, did, time_us, commit)

      {@grant_collection, "delete"} ->
        {:delete_grant, %{uri: uri, time_us: time_us}}

      {@keyring_collection, op} when op in ["create", "update"] ->
        parse_keyring_upsert(uri, did, time_us, commit)

      {@keyring_collection, "delete"} ->
        {:delete_keyring, %{uri: uri, time_us: time_us}}

      _ ->
        :ignore
    end
  end

  defp parse_commit(_, _, _), do: :ignore

  defp parse_grant_upsert(uri, did, time_us, %{"record" => record}) when is_map(record) do
    recipient = record["recipient"]
    document = record["document"]
    created_at = record["createdAt"]

    if is_binary(recipient) and is_binary(document) and is_binary(created_at) do
      {:upsert_grant,
       %{
         uri: uri,
         owner_did: did,
         recipient_did: recipient,
         document_uri: document,
         created_at: created_at,
         time_us: time_us
       }}
    else
      :ignore
    end
  end

  defp parse_grant_upsert(_, _, _, _), do: :ignore

  defp parse_keyring_upsert(uri, did, time_us, %{"record" => record}) when is_map(record) do
    members = record["members"] || []

    member_dids =
      members
      |> Enum.filter(&is_map/1)
      |> Enum.map(& &1["did"])
      |> Enum.filter(&is_binary/1)

    {:upsert_keyring,
     %{
       uri: uri,
       owner_did: did,
       member_dids: member_dids,
       time_us: time_us
     }}
  end

  defp parse_keyring_upsert(_, _, _, _), do: :ignore
end
