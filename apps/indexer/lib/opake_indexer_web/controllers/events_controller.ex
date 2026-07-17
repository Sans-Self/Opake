defmodule OpakeIndexerWeb.EventsController do
  @moduledoc """
  SSE event streaming + token exchange endpoint.

  ## Token exchange

  `POST /api/events/token` (Ed25519-authenticated) returns a short-lived
  single-use opaque token. The client passes it as `?token=` on the SSE
  endpoint, sidestepping EventSource's lack of custom headers.

  ## SSE stream

  `GET /api/events?token=<token>` validates the token, subscribes to
  PubSub topics for the authenticated DID, and enters a chunked response
  loop. Each event is forwarded as an SSE message. A keepalive comment
  fires every 15 seconds.

  Payloads on the wire are envelope-shaped for record events
  (`%{record, indexedAt}`) and flat for notifications (`chain:forked`).
  """

  use OpakeIndexerWeb, :controller

  require Logger

  alias OpakeIndexer.SSE.{TokenStore, ConnectionTracker, Topics}
  alias OpakeIndexer.Queries.RecordQueries

  @keepalive_interval_ms 15_000
  @pubsub OpakeIndexer.PubSub

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

  # -- SSE loop -------------------------------------------------------

  defp subscribe_and_loop(conn, did) do
    Phoenix.PubSub.subscribe(@pubsub, Topics.personal(did))

    # Subscribe to every workspace the DID is currently a member of.
    # Workspace topics are keyed by workspace_id (= genesis keyring URI).
    keyrings = RecordQueries.workspaces_for_member(did)

    workspace_ids =
      keyrings
      |> Enum.map(& &1.workspace_id)
      |> Enum.reject(&is_nil/1)
      |> MapSet.new()

    Enum.each(workspace_ids, fn workspace_id ->
      Phoenix.PubSub.subscribe(@pubsub, Topics.workspace(workspace_id))
    end)

    timer = schedule_keepalive()
    state = %{did: did, subscribed_workspaces: workspace_ids, keepalive_timer: timer}

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
      {:sse_event, "at.opake.keyring:upsert" = event_type, payload} ->
        state = handle_keyring_membership(payload, state)

        case send_sse_event(conn, event_type, payload) do
          {:ok, conn} -> sse_loop(conn, state)
          {:error, _} -> conn
        end

      {:sse_event, "at.opake.keyring:delete", payload} ->
        # Delete payload is `%{uri}` — we can't tell from it which workspace
        # this keyring belonged to (the record is already gone). Leave the
        # subscription in place; it'll silently no-op once the workspace
        # topic stops receiving events.
        case send_sse_event(conn, "at.opake.keyring:delete", payload) do
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
            conn
        end
    end
  end

  # -- Dynamic membership management ----------------------------------
  #
  # When a keyring supersede arrives that adds or removes us, we add
  # or drop the corresponding workspace topic subscription. The
  # `workspace_id` is read off the envelope's record — it identifies
  # the workspace this keyring belongs to.

  defp handle_keyring_membership(payload, state) do
    record = record_from_envelope(payload)
    # workspace_id resolution for keyring records:
    #
    #   * Superseded keyrings carry `lineage` in their record body
    #     (the genesis URI of the workspace they belong to).
    #
    #   * Genesis keyrings have NO `lineage` field — they ARE the
    #     workspace_id. Their own AT URI is the genesis URI, which lives
    #     on the envelope's top-level `uri`, NOT in the record body
    #     (records don't carry their own URI). Reading `record["uri"]`
    #     here was always nil and silently dropped genesis subscriptions
    #     for the creator's open SSE session — uploads to a freshly-
    #     created workspace wouldn't surface until reconnect.
    workspace_id = record["lineage"] || envelope_uri(payload)

    member_dids =
      (record["members"] || [])
      |> Enum.map(fn entry -> get_in(entry, ["wrappedKey", "did"]) end)
      |> Enum.reject(&is_nil/1)
      |> MapSet.new()

    is_member = MapSet.member?(member_dids, state.did)
    was_subscribed = is_binary(workspace_id) and MapSet.member?(state.subscribed_workspaces, workspace_id)

    cond do
      is_member and is_binary(workspace_id) and not was_subscribed ->
        Phoenix.PubSub.subscribe(@pubsub, Topics.workspace(workspace_id))
        %{state | subscribed_workspaces: MapSet.put(state.subscribed_workspaces, workspace_id)}

      not is_member and was_subscribed ->
        Phoenix.PubSub.unsubscribe(@pubsub, Topics.workspace(workspace_id))
        %{state | subscribed_workspaces: MapSet.delete(state.subscribed_workspaces, workspace_id)}

      true ->
        state
    end
  end

  defp record_from_envelope(%{record: r}) when is_map(r), do: r
  defp record_from_envelope(%{"record" => r}) when is_map(r), do: r
  defp record_from_envelope(_), do: %{}

  defp envelope_uri(%{uri: u}) when is_binary(u), do: u
  defp envelope_uri(%{"uri" => u}) when is_binary(u), do: u
  defp envelope_uri(_), do: nil

  # -- Helpers --------------------------------------------------------

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
