defmodule OpakeIndexerWeb.Plugs.RateLimit do
  @moduledoc """
  Per-IP rate limiting via Hammer (ETS backend). Allows 30 requests per second
  burst. Respects `X-Forwarded-For` and `X-Real-IP` headers for clients behind
  a reverse proxy.
  """

  import Plug.Conn

  @behaviour Plug

  @burst_size 30
  @scale_ms :timer.seconds(1)

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    client_ip = client_ip(conn)
    bucket = "rate_limit:#{client_ip}"

    case Hammer.check_rate(bucket, @scale_ms, @burst_size) do
      {:allow, _count} ->
        conn

      {:deny, _limit} ->
        conn
        |> put_status(429)
        |> Phoenix.Controller.json(%{error: "rate limit exceeded"})
        |> halt()
    end
  end

  defp client_ip(conn) do
    forwarded_for =
      get_req_header(conn, "x-forwarded-for")
      |> List.first()

    real_ip = get_req_header(conn, "x-real-ip") |> List.first()

    cond do
      forwarded_for ->
        forwarded_for |> String.split(",") |> List.first() |> String.trim()

      real_ip ->
        real_ip

      true ->
        conn.remote_ip |> :inet.ntoa() |> to_string()
    end
  end
end
