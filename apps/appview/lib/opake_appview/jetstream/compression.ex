defmodule OpakeAppview.Jetstream.Compression do
  @moduledoc """
  zstandard decompression for the Jetstream binary frame protocol.

  ## Why this exists

  The Bluesky Jetstream firehose ships ~500 GB/month of uncompressed JSON.
  Enabling its built-in compression (`?compress=true`) cuts that by ~56%.
  Jetstream uses zstd with a precomputed dictionary trained on bsky's
  record shapes — the dictionary is required to decompress any frame.

  ## Dictionary lifecycle

  The dictionary is vendored at `priv/jetstream_zstd_dict` and read at
  module-compile time via `@external_resource`. At runtime, an `:ezstd`
  decompression dictionary (a NIF resource) is built once and stashed in
  `:persistent_term` so every frame decompression skips the construction
  cost. This is safe because `ezstd` ddicts are immutable + thread-safe
  by design.

  ## Dict ID drift detection

  Both the local dictionary and every compressed frame carry a 32-bit
  dictionary ID. If they don't match, decompression silently produces
  garbage (or noisy errors at best). We hardcode the expected dict ID
  here and provide `validate_frame!/1` to assert that incoming frames
  match. The consumer calls this on the first frame after connecting
  and crashes loudly with a clear message if the IDs disagree —
  much better than mysterious decode failures downstream.

  Bumping the dictionary means: re-vendor `priv/jetstream_zstd_dict`
  and update `@expected_dict_id` to match. The compile-time check in
  `init/0` will catch any mismatch immediately on app boot.
  """

  require Logger

  @dict_path Application.app_dir(:opake_appview, "priv/jetstream_zstd_dict")
  @external_resource @dict_path
  @dict_bytes File.read!(@dict_path)

  # The Bluesky Jetstream zstd dictionary published at
  # github.com/bluesky-social/jetstream/blob/main/pkg/models/zstd_dictionary.
  # If bsky rotates the dictionary, this constant must be bumped (and the
  # vendored file replaced) — `init/0` will refuse to start otherwise.
  @expected_dict_id 1_612_007_021

  @persistent_term_key {__MODULE__, :ddict}

  # Streaming decompression buffer (256 KiB). Sized to comfortably hold any
  # single Jetstream commit — the largest bsky records compress well below
  # this. The context is reused per-consumer to amortize NIF setup cost.
  @stream_buffer_size 262_144

  # -- Public API --

  @doc """
  Initializes the persistent decompression dictionary. Idempotent.

  Verifies that the vendored dictionary's embedded ID matches
  `@expected_dict_id` and raises a clear error if it doesn't (which
  would indicate a vendoring mistake or a stale build artifact).

  Called from `OpakeAppview.Application.start/2` so the ddict is ready
  before the consumer connects.
  """
  @spec init() :: :ok
  def init do
    ddict = :ezstd.create_ddict(@dict_bytes)
    actual_id = :ezstd.get_dict_id_from_ddict(ddict)

    if actual_id != @expected_dict_id do
      raise """
      Jetstream zstd dictionary ID mismatch.

        expected: #{@expected_dict_id}
        actual:   #{actual_id}

      The vendored dictionary at #{@dict_path} does not match the ID
      hardcoded in #{inspect(__MODULE__)}. This usually means:

        1. The dictionary file was re-vendored without updating
           @expected_dict_id, OR
        2. A stale dictionary file is left over from an old build.

      Re-download the dictionary from
        https://raw.githubusercontent.com/bluesky-social/jetstream/main/pkg/models/zstd_dictionary
      and update @expected_dict_id to #{actual_id}.
      """
    end

    :persistent_term.put(@persistent_term_key, ddict)
    Logger.info("[Compression] zstd dictionary loaded (id=#{actual_id})")
    :ok
  end

  @doc """
  Creates a streaming decompression context with the Jetstream ddict
  pre-selected. One context per consumer process.

  We use streaming decompression because Jetstream's compressed frames
  do NOT carry an explicit decompressed-size field in their zstd header.
  `decompress_using_ddict/2` calls `ZSTD_getFrameContentSize` to size the
  output buffer up front and bails with `ZSTD_CONTENTSIZE_UNKNOWN`. The
  streaming API processes incrementally and doesn't need the size hint.
  """
  @spec new_context() :: reference()
  def new_context do
    ctx = :ezstd.create_decompression_context(@stream_buffer_size)
    :ok = :ezstd.select_ddict(ctx, ddict())
    ctx
  end

  @doc """
  Decompresses a single zstd frame using a previously-created streaming
  context. The session is reset after each call so subsequent frames
  start clean.

  Returns the decompressed binary on success, or `{:error, reason}`
  if the frame can't be decoded. Callers should treat errors as fatal
  — a single bad frame indicates protocol drift, not transient noise.
  """
  @spec decompress(reference(), binary()) :: {:ok, binary()} | {:error, term()}
  def decompress(context, frame) when is_reference(context) and is_binary(frame) do
    result =
      case :ezstd.decompress_streaming(context, frame) do
        {:error, _} = err ->
          err

        iolist when is_list(iolist) ->
          {:ok, IO.iodata_to_binary(iolist)}

        other ->
          {:error, {:unexpected, other}}
      end

    # Reset between frames so internal state from a previous frame
    # cannot bleed into the next one. Keeps the ddict selection.
    :ezstd.reset_decompression_context(context, :session_only)
    result
  end

  @doc """
  Asserts that a compressed frame's embedded dict ID matches the
  expected ID. Used by the consumer on the first received frame to
  catch dict drift early — before we try to decompress with a wrong
  key and fail mysteriously downstream.

  Crashes the calling process on mismatch with a clear error message.
  """
  @spec validate_frame!(binary()) :: :ok
  def validate_frame!(frame) when is_binary(frame) do
    case :ezstd.get_dict_id_from_frame(frame) do
      0 ->
        # 0 means "no dictionary used" — frame was compressed without
        # a dict. Could happen if the server somehow disabled it. We
        # accept this and let decompression proceed without a ddict.
        Logger.warning(
          "[Compression] received uncompressed frame from Jetstream " <>
            "(dict_id=0) — server may have disabled compression"
        )

        :ok

      id when id == @expected_dict_id ->
        :ok

      mismatched ->
        raise """
        Jetstream frame uses an unexpected zstd dictionary.

          expected: #{@expected_dict_id}
          received: #{mismatched}

        Bluesky has likely rotated the Jetstream compression dictionary.
        Re-vendor priv/jetstream_zstd_dict from
          https://raw.githubusercontent.com/bluesky-social/jetstream/main/pkg/models/zstd_dictionary
        and update @expected_dict_id in #{inspect(__MODULE__)} to #{mismatched}.
        """
    end
  end

  @doc "Returns the dict ID this build was compiled against. For diagnostics."
  @spec expected_dict_id() :: integer()
  def expected_dict_id, do: @expected_dict_id

  # -- Internal --

  defp ddict do
    case :persistent_term.get(@persistent_term_key, :not_loaded) do
      :not_loaded ->
        # Lazy-init fallback for test environments that bypass the
        # application supervisor. Production hits init/0 once at boot.
        init()
        :persistent_term.get(@persistent_term_key)

      ddict ->
        ddict
    end
  end
end
