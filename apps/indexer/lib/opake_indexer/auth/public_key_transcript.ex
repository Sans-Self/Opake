defmodule OpakeIndexer.Auth.PublicKeyTranscript do
  @moduledoc false

  @spec signature(pos_integer(), String.t(), [binary()]) :: binary()
  def signature(version, did, [
        x25519,
        x_algo,
        ml_kem,
        ml_kem_algo,
        signing,
        signing_algo,
        created_at
      ]) do
    context("at.opake.publicKey/self:v#{version}", [
      did,
      <<version::little-unsigned-32>>,
      x25519,
      x_algo,
      ml_kem,
      ml_kem_algo,
      signing,
      signing_algo,
      created_at
    ])
  end

  @spec context(String.t(), [binary()]) :: binary()
  def context(label, fields) do
    [label, <<length(fields)::little-unsigned-32>> | Enum.map(fields, &frame/1)]
    |> IO.iodata_to_binary()
  end

  defp frame(field), do: [<<byte_size(field)::little-unsigned-32>>, field]
end
