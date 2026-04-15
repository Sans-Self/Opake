defmodule OpakeAppviewWeb.EventsController do
  @moduledoc """
  SSE event streaming + token exchange endpoint.

  ## Token exchange

  `POST /api/events/token` (Ed25519-authenticated) returns a short-lived
  single-use opaque token. The client passes it as `?token=` on the SSE
  endpoint, sidestepping EventSource's lack of custom headers.

  ## SSE stream

  `GET /api/events?token=<token>` validates the token, subscribes to
  PubSub topics for the authenticated DID, and enters a chunked response
  loop. Each indexed event is forwarded as an SSE message. A keepalive
  comment is sent every 15 seconds to prevent proxy timeouts.
  """

  use OpakeAppviewWeb, :controller

  require Logger

  alias OpakeAppview.SSE.{TokenStore, ConnectionTracker, Topics}
  alias OpakeAppview.Queries.KeyringQueries

  @keepalive_interval_ms 15_000
  @pubsub OpakeAppview.PubSub

  # -- Token exchange --

  @spec create_token(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def create_token(conn, _params) do
    did = conn.assigns.authenticated_did
    token = TokenStore.create_token(did)
    json(conn, %{token: token, ttl: TokenStore.ttl_seconds()})
  end

  # -- SSE stream --

  @spec stream(Plug.Conn.t(), map()) :: Plug.Conn.t()
  def stream(conn, %{"token" => token}) do
    case TokenStore.consume_token(token) do
      {:ok, did} ->
        case ConnectionTracker.acquire(did) do
          :ok ->
            conn
            |> put_resp_header("content-type", "text/event-stream")
            |> put_resp_header("cache-control", "no-cache")
            |> put_resp_header("connection", "keep-alive")
            |> put_resp_header("x-accel-buffering", "no")
            |> send_chunked(200)
            |> subscribe_and_loop(did)

          {:error, :limit_reached} ->
            conn
            |> put_status(429)
            |> json(%{error: "too many SSE connections for this account"})
        end

      :error ->
        conn
        |> put_status(401)
        |> json(%{error: "invalid or expired token"})
    end
  end

  def stream(conn, _params) do
    conn
    |> put_status(400)
    |> json(%{error: "token parameter required"})
  end

  # -- SSE loop --

  defp subscribe_and_loop(conn, did) do
    # Subscribe to personal topic
    Phoenix.PubSub.subscribe(@pubsub, Topics.personal(did))

    # Subscribe to all workspace keyrings
    {keyrings, _cursor} = KeyringQueries.list_keyrings_for_member(did, limit: 1000)
    keyring_uris = MapSet.new(Enum.map(keyrings, & &1.uri))

    if MapSet.size(keyring_uris) >= 1000 do
      Logger.warning("[SSE] #{did} has 1000+ keyrings — SSE subscriptions truncated")
    end

    Enum.each(keyring_uris, fn uri ->
      Phoenix.PubSub.subscribe(@pubsub, Topics.workspace(uri))
    end)

    timer = schedule_keepalive()
    state = %{did: did, subscribed_keyrings: keyring_uris, keepalive_timer: timer}

    # try/after guarantees ConnectionTracker.release even on crash.
    # Phoenix requires the action to return a Plug.Conn, so we track it
    # through the loop and return it after cleanup.
    final_conn =
      try do
        sse_loop(conn, state)
      after
        cleanup(state)
      end

    final_conn
  end

  defp sse_loop(conn, state) do
    receive do
      {:sse_event, "keyring:upsert" = event_type, payload} ->
        # Dynamic membership: check if we need to subscribe/unsubscribe
        state = handle_keyring_membership(payload, state)

        case send_sse_event(conn, event_type, payload) do
          {:ok, conn} -> sse_loop(conn, state)
          {:error, _} -> conn
        end

      {:sse_event, "keyring:delete", payload} ->
        # Unsubscribe from deleted keyring
        uri = payload[:uri] || payload["uri"]
        state = unsubscribe_keyring(uri, state)

        case send_sse_event(conn, "keyring:delete", payload) do
          {:ok, conn} -> sse_loop(conn, state)
          {:error, _} -> conn
        end

      {:sse_event, event_type, payload} ->
        case send_sse_event(conn, event_type, payload) do
          {:ok, conn} -> sse_loop(conn, state)
          {:error, _} -> conn
        end

      :keepalive ->
        case Plug.Conn.chunk(conn, ": keepalive\n\n") do
          {:ok, conn} ->
            timer = schedule_keepalive()
            sse_loop(conn, %{state | keepalive_timer: timer})

          {:error, _} ->
            :ok
        end
    end
  end

  # -- Dynamic membership management --

  defp handle_keyring_membership(payload, state) do
    uri = payload[:uri] || payload["uri"]
    members = payload[:member_entries] || payload["member_entries"] || []

    member_dids =
      members
      |> Enum.map(fn
        %{did: did} -> did
        %{"did" => did} -> did
        _ -> nil
      end)
      |> Enum.reject(&is_nil/1)
      |> MapSet.new()

    is_member = MapSet.member?(member_dids, state.did)
    was_subscribed = uri && MapSet.member?(state.subscribed_keyrings, uri)

    cond do
      # New membership — subscribe.
      # `uri` can be nil (malformed keyring event), so gate via `is_nil/1`
      # rather than using it as a truthy test in `and`, which demands a
      # boolean on both sides and crashes on raw strings.
      is_member and not is_nil(uri) and not was_subscribed ->
        Phoenix.PubSub.subscribe(@pubsub, Topics.workspace(uri))
        %{state | subscribed_keyrings: MapSet.put(state.subscribed_keyrings, uri)}

      # Removed from membership — unsubscribe
      not is_member and was_subscribed ->
        unsubscribe_keyring(uri, state)

      true ->
        state
    end
  end

  defp unsubscribe_keyring(nil, state), do: state

  defp unsubscribe_keyring(uri, state) do
    Phoenix.PubSub.unsubscribe(@pubsub, Topics.workspace(uri))
    %{state | subscribed_keyrings: MapSet.delete(state.subscribed_keyrings, uri)}
  end

  # -- Helpers --

  defp send_sse_event(conn, event_type, payload) do
    data = Jason.encode!(payload)
    Plug.Conn.chunk(conn, "event: #{event_type}\ndata: #{data}\n\n")
  end

  defp schedule_keepalive do
    Process.send_after(self(), :keepalive, @keepalive_interval_ms)
  end

  defp cleanup(state) do
    if timer = state[:keepalive_timer], do: Process.cancel_timer(timer)
    ConnectionTracker.release(state.did)
    :ok
  end
end
