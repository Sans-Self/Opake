defmodule OpakeIndexer.Auth.Plug do
  @moduledoc """
  Plug that verifies `Opake-Ed25519` authentication headers.

  Header format: `Opake-Ed25519 <did>:<unix-timestamp>:<base64-signature>`
  Signature covers: `<METHOD>:<path>:<timestamp>:<did>`

  Parsing splits from the right because DIDs contain colons. Enforces a 60-second
  replay window and optional `?did=` scope check. The Ed25519 public key is
  resolved via the KeyCache (which fetches from the user's PDS on cache miss).
  """

  import Plug.Conn

  @behaviour Plug

  @max_timestamp_drift_secs 60
  @auth_prefix "Opake-Ed25519 "

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    case authenticate(conn) do
      {:ok, did, verified, anchor_history} ->
        conn
        |> assign(:authenticated_did, did)
        |> assign(:authenticated_verified, verified)
        |> assign(:authenticated_anchor_history, anchor_history)

      {:error, {:unavailable, _source}} ->
        conn
        |> put_status(503)
        |> Phoenix.Controller.json(%{error: "authentication key resolution unavailable"})
        |> halt()

      {:error, {:invalid, _reason}} ->
        conn
        |> put_status(401)
        |> Phoenix.Controller.json(%{error: "authentication key resolution invalid"})
        |> halt()

      {:error, message} when is_binary(message) ->
        conn
        |> put_status(401)
        |> Phoenix.Controller.json(%{error: message})
        |> halt()

      {:error, _reason} ->
        conn
        |> put_status(401)
        |> Phoenix.Controller.json(%{error: "authentication failed"})
        |> halt()
    end
  end

  defp authenticate(conn) do
    with {:ok, payload} <- extract_auth_header(conn),
         {:ok, did, timestamp_str, signature_b64} <- parse_auth_payload(payload),
         {:ok, timestamp} <- parse_timestamp(timestamp_str),
         :ok <- check_timestamp_drift(timestamp),
         :ok <- check_did_scope(conn, did),
         {:ok, signature} <- OpakeIndexer.Auth.Base64.decode(signature_b64),
         {:ok, %{key: pubkey, verified: verified, anchor_history: anchor_history}} <-
           OpakeIndexer.Auth.KeyCache.get_decision(did) do
      method = conn.method
      path = conn.request_path
      message = "#{method}:#{path}:#{timestamp}:#{did}"

      case verify_signature(pubkey, message, signature) do
        :ok -> {:ok, did, verified, anchor_history}
        {:error, _} = err -> err
      end
    end
  end

  defp extract_auth_header(conn) do
    case get_req_header(conn, "authorization") do
      [header] ->
        if String.starts_with?(header, @auth_prefix) do
          {:ok, String.trim_leading(header, @auth_prefix)}
        else
          {:error, "invalid authorization scheme"}
        end

      [] ->
        {:error, "missing authorization header"}

      _ ->
        {:error, "multiple authorization headers"}
    end
  end

  # Parse from right: DIDs contain colons, so we reverse-split to separate
  # the signature (last), timestamp (second-to-last), and DID (everything else)
  defp parse_auth_payload(payload) do
    parts = String.split(payload, ":") |> Enum.reverse()

    case parts do
      [signature, timestamp | did_parts] when did_parts != [] ->
        did = did_parts |> Enum.reverse() |> Enum.join(":")
        {:ok, did, timestamp, signature}

      _ ->
        {:error, "malformed auth payload"}
    end
  end

  defp parse_timestamp(timestamp_str) do
    case Integer.parse(timestamp_str) do
      {ts, ""} -> {:ok, ts}
      _ -> {:error, "invalid timestamp"}
    end
  end

  defp check_timestamp_drift(timestamp) do
    now = System.system_time(:second)
    drift = abs(now - timestamp)

    if drift <= @max_timestamp_drift_secs do
      :ok
    else
      {:error, "timestamp drift too large (#{drift}s)"}
    end
  end

  defp check_did_scope(conn, authenticated_did) do
    case conn.query_params["did"] do
      nil -> :ok
      did when did == authenticated_did -> :ok
      _ -> {:error, "DID scope mismatch"}
    end
  end

  defp verify_signature(pubkey, message, signature) do
    case :crypto.verify(:eddsa, :none, message, signature, [pubkey, :ed25519]) do
      true -> :ok
      false -> {:error, "invalid signature"}
    end
  rescue
    _ -> {:error, "signature verification failed"}
  end
end
