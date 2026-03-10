defmodule OpakeAppview.Auth.Base64 do
  @moduledoc """
  Flexible base64 decoding that handles both padded and unpadded input.
  AT Protocol encodes keys and signatures inconsistently.
  """

  def decode(str) do
    case Base.decode64(str) do
      {:ok, bytes} ->
        {:ok, bytes}

      :error ->
        case Base.decode64(str, padding: false) do
          {:ok, bytes} -> {:ok, bytes}
          :error -> {:error, "invalid base64 encoding"}
        end
    end
  end
end
