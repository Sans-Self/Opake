defmodule Mix.Tasks.Opake.Tail do
  @shortdoc "Tail the Jetstream firehose. Prints every received frame."

  @moduledoc """
  Diagnostic dev task. Connects to Jetstream using the configured
  `:jetstream_url` and `:firehose_mode`, prints every received frame
  to stdout, and exits on Ctrl-C.

  Does NOT touch Postgres, the indexer, or the cursor table. Strictly
  for "is the firehose alive right now and what's it sending me" use.

  ## Examples

      mix opake.tail
      mix opake.tail --mode opake_only
      mix opake.tail --collection app.opake.document --collection app.opake.grant
      mix opake.tail --pretty
      mix opake.tail --max 50
      mix opake.tail --no-compression

  ## Options

    * `--mode MODE` — `full` (default), `opake_only`, or `custom`. If
      `custom`, you must pass at least one `--collection`.
    * `--collection NAME` — repeatable. Implies `--mode custom`.
    * `--pretty` — pretty-print JSON instead of one line per event.
    * `--max N` — exit after N frames (default: unlimited).
    * `--no-compression` — request raw JSON frames instead of zstd.
      Use to debug if you suspect the dictionary is broken.
  """

  use Mix.Task

  alias OpakeAppview.Jetstream.{Compression, Event}

  @opake_collections [
    "app.opake.grant",
    "app.opake.keyring",
    "app.opake.document",
    "app.opake.documentUpdate",
    "app.opake.keyringUpdate",
    "app.opake.directory",
    "app.opake.directoryUpdate"
  ]

  @impl Mix.Task
  def run(argv) do
    {opts, _, _} =
      OptionParser.parse(argv,
        strict: [
          mode: :string,
          collection: [:string, :keep],
          pretty: :boolean,
          max: :integer,
          compression: :boolean
        ]
      )

    Mix.Task.run("app.config")
    Application.ensure_all_started(:websockex)
    Application.ensure_all_started(:jason)
    Application.ensure_all_started(:ezstd)

    compression =
      case Keyword.get(opts, :compression, true) do
        true -> :zstd
        false -> :none
      end

    if compression == :zstd, do: Compression.init()

    decompression_context =
      if compression == :zstd do
        Compression.new_context()
      else
        nil
      end

    mode = resolve_mode(opts)
    url = build_url(mode, compression)
    max = opts[:max]
    pretty? = opts[:pretty] == true

    Mix.shell().info("[opake.tail] mode=#{inspect(mode)} compression=#{compression}")
    Mix.shell().info("[opake.tail] connecting to #{url}")

    {:ok, _pid} =
      OpakeAppview.Mix.Tail.Listener.start_link(%{
        url: url,
        max: max,
        pretty?: pretty?,
        compression: compression,
        decompression_context: decompression_context,
        printed: 0,
        parent: self()
      })

    receive do
      :done -> Mix.shell().info("[opake.tail] reached --max, exiting")
      {:exit, reason} -> Mix.raise("[opake.tail] connection closed: #{inspect(reason)}")
    end
  end

  defp resolve_mode(opts) do
    collections = Keyword.get_values(opts, :collection)

    case {opts[:mode], collections} do
      {nil, []} -> :full
      {"full", _} -> :full
      {"opake_only", _} -> :opake_only
      {"custom", []} -> Mix.raise("--mode custom requires at least one --collection")
      {"custom", list} -> {:custom, list}
      {nil, list} when list != [] -> {:custom, list}
      {other, _} -> Mix.raise("Unknown --mode: #{other}")
    end
  end

  defp build_url(mode, compression) do
    base = Application.fetch_env!(:opake_appview, :jetstream_url)

    params =
      mode
      |> collections_for()
      |> Enum.map(&"wantedCollections=#{&1}")
      |> Enum.concat(compression_param(compression))
      |> Enum.join("&")

    case params do
      "" -> base
      _ -> "#{base}?#{params}"
    end
  end

  defp collections_for(:full), do: []
  defp collections_for(:opake_only), do: @opake_collections
  defp collections_for({:custom, list}), do: list

  defp compression_param(:zstd), do: ["compress=true"]
  defp compression_param(:none), do: []

  @doc false
  def format_frame(json, pretty?) do
    case Event.parse(json) do
      {time_us, collection, payload} ->
        tag = format_tag(payload)
        time = format_time(time_us)
        coll = collection || "?"

        if pretty? do
          decoded = Jason.decode!(json)

          [
            "\n[#{time}] #{coll} #{tag}\n",
            Jason.encode_to_iodata!(decoded, pretty: true),
            "\n"
          ]
        else
          ["[#{time}] #{String.pad_trailing(coll, 32)} #{tag}\n"]
        end
    end
  end

  defp format_tag(:ignore), do: "ignored"
  defp format_tag({tag, _}), do: Atom.to_string(tag)

  defp format_time(nil), do: "-------"

  defp format_time(time_us) when is_integer(time_us) do
    time_us
    |> DateTime.from_unix!(:microsecond)
    |> Calendar.strftime("%H:%M:%S")
  end
end

defmodule OpakeAppview.Mix.Tail.Listener do
  @moduledoc false
  # WebSockex client used by `mix opake.tail`. Defined in the same file as
  # the task because it has no purpose outside it.

  use WebSockex

  alias OpakeAppview.Jetstream.Compression

  def start_link(state) do
    WebSockex.start_link(state.url, __MODULE__, state)
  end

  def handle_connect(_conn, state) do
    Mix.shell().info("[opake.tail] connected\n")
    {:ok, state}
  end

  def handle_frame({:text, msg}, state) do
    print_and_continue(msg, state)
  end

  def handle_frame({:binary, msg}, %{compression: :zstd} = state) do
    case Compression.decompress(state.decompression_context, msg) do
      {:ok, json} ->
        print_and_continue(json, state)

      {:error, reason} ->
        Mix.shell().error("[opake.tail] decompress failed: #{inspect(reason)}")
        send(state.parent, {:exit, {:decompress_failed, reason}})
        {:close, state}
    end
  end

  def handle_frame(_, state), do: {:ok, state}

  def handle_disconnect(%{reason: reason}, state) do
    send(state.parent, {:exit, reason})
    {:ok, state}
  end

  defp print_and_continue(json, state) do
    IO.write(Mix.Tasks.Opake.Tail.format_frame(json, state.pretty?))
    new_printed = state.printed + 1

    if state.max && new_printed >= state.max do
      send(state.parent, :done)
      {:close, %{state | printed: new_printed}}
    else
      {:ok, %{state | printed: new_printed}}
    end
  end
end
