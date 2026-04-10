defmodule Mix.Tasks.Opake.Resync do
  @moduledoc """
  Backfill a DID's `app.opake.*` records from their PDS into the appview DB.

  Fetches keyrings, directories, documents, and grants via the public
  `com.atproto.repo.listRecords` endpoint and upserts them through the
  same query path as the firehose indexer. Useful after a DB wipe, stale
  cursor, or when the firehose missed events.

  ## Usage

      mix opake.resync did:plc:wydyrngmxbcsqdvhmd7whmye
      mix opake.resync sans-self.org
      mix opake.resync --all

  `--all` backfills every DID found in the `keyring_members` table (i.e.,
  every DID the appview has ever seen as a workspace member).
  """

  use Mix.Task

  @shortdoc "Backfill opake records from a PDS into the appview"

  @impl Mix.Task
  def run(argv) do
    {opts, args, _} =
      OptionParser.parse(argv,
        strict: [all: :boolean]
      )

    Mix.Task.run("app.start")

    cond do
      opts[:all] ->
        OpakeAppview.Backfill.backfill_known_dids()
        Mix.shell().info("Backfill complete for all known DIDs.")

      length(args) == 1 ->
        identifier = hd(args)
        did = resolve_identifier(identifier)

        case OpakeAppview.Backfill.backfill_did(did) do
          :ok ->
            Mix.shell().info("Backfill complete for #{did}.")

          {:error, reason} ->
            Mix.raise("Backfill failed for #{did}: #{inspect(reason)}")
        end

      true ->
        Mix.raise("""
        Usage: mix opake.resync <did-or-handle>
               mix opake.resync --all
        """)
    end
  end

  defp resolve_identifier("did:" <> _ = did), do: did

  defp resolve_identifier(handle) do
    Mix.shell().info("Resolving handle #{handle}...")

    case :inet_res.lookup(~c"_atproto.#{handle}", :in, :txt) do
      [[record] | _] ->
        txt = to_string(record)

        case String.split(txt, "=", parts: 2) do
          ["did", did] ->
            Mix.shell().info("Resolved #{handle} → #{did}")
            did

          _ ->
            Mix.raise("Could not resolve handle #{handle}: unexpected TXT record #{txt}")
        end

      _ ->
        Mix.raise("Could not resolve handle #{handle}: no DNS TXT record at _atproto.#{handle}")
    end
  end
end
