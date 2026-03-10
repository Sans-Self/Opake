defmodule OpakeAppviewWeb.KeyringsControllerTest do
  use OpakeAppviewWeb.ConnCase, async: false
  import Mox

  alias OpakeAppview.Queries.KeyringQueries

  setup :set_mox_global
  setup :verify_on_exit!

  setup do
    :ets.delete_all_objects(:key_cache)
    :ok
  end

  test "requires did parameter", %{conn: conn} do
    did = "did:plc:test"
    conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

    response = json_response(conn, 400)
    assert response["error"] =~ "did"
  end

  test "returns memberships", %{conn: conn} do
    did = "did:plc:me"

    {:ok, _} =
      KeyringQueries.upsert_keyring(
        "at://did:plc:owner/app.opake.keyring/3def",
        "did:plc:owner",
        [did, "did:plc:other"]
      )

    conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings?did=#{did}")

    response = json_response(conn, 200)
    assert length(response["keyrings"]) == 1

    keyring = hd(response["keyrings"])
    assert keyring["ownerDid"] == "did:plc:owner"
  end
end
