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

  @keyring_uri "at://did:plc:owner/app.opake.keyring/3def"

  defp setup_keyring(member_did) do
    wk_me = %{
      "did" => member_did,
      "ciphertext" => %{"$bytes" => "AAAA"},
      "algo" => "x25519-hkdf-a256kw"
    }

    wk_other = %{
      "did" => "did:plc:other",
      "ciphertext" => %{"$bytes" => "BBBB"},
      "algo" => "x25519-hkdf-a256kw"
    }

    {:ok, _} =
      KeyringQueries.upsert_keyring(
        @keyring_uri,
        "did:plc:owner",
        [
          %{did: member_did, role: "manager", wrapped_key: wk_me},
          %{did: "did:plc:other", role: "editor", wrapped_key: wk_other}
        ]
      )

    {:ok, _} =
      KeyringQueries.upsert_keyring_record(%{
        uri: @keyring_uri,
        owner_did: "did:plc:owner",
        rotation: 2,
        encrypted_metadata: %{"ciphertext" => "CCCC", "nonce" => "DDDD"},
        created_at: "2026-03-01T12:00:00Z"
      })

    {wk_me, wk_other}
  end

  test "returns full keyring data for members", %{conn: conn} do
    did = "did:plc:me"
    {wk_me, wk_other} = setup_keyring(did)

    conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

    response = json_response(conn, 200)
    assert length(response["keyrings"]) == 1

    keyring = hd(response["keyrings"])
    assert keyring["uri"] == @keyring_uri
    assert keyring["owner_did"] == "did:plc:owner"
    assert keyring["rotation"] == 2
    assert keyring["encrypted_metadata"] == %{"ciphertext" => "CCCC", "nonce" => "DDDD"}
    assert keyring["created_at"] == "2026-03-01T12:00:00Z"
    assert is_binary(keyring["indexed_at"])

    # Members reconstructed from keyring_members join
    members = keyring["members"]
    assert length(members) == 2

    by_role = Enum.group_by(members, & &1["role"])
    assert hd(by_role["manager"])["wrappedKey"] == wk_me
    assert hd(by_role["editor"])["wrappedKey"] == wk_other
  end

  test "returns empty list for non-member", %{conn: conn} do
    did = "did:plc:outsider"
    setup_keyring("did:plc:someone-else")

    conn = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings")

    response = json_response(conn, 200)
    assert response["keyrings"] == []
  end

  test "paginates with cursor", %{conn: conn} do
    did = "did:plc:me"

    for i <- 1..3 do
      uri = "at://did:plc:owner/app.opake.keyring/#{i}"

      {:ok, _} =
        KeyringQueries.upsert_keyring(uri, "did:plc:owner", [
          %{did: did, role: "manager", wrapped_key: %{"did" => did}}
        ])

      {:ok, _} =
        KeyringQueries.upsert_keyring_record(%{
          uri: uri,
          owner_did: "did:plc:owner",
          rotation: 0,
          created_at: "2026-03-01T12:00:00Z"
        })
    end

    conn1 = conn |> authed_conn(did, "/api/keyrings") |> get("/api/keyrings?limit=2")
    response1 = json_response(conn1, 200)
    assert length(response1["keyrings"]) == 2
    assert is_binary(response1["cursor"])

    :ets.delete_all_objects(:key_cache)

    conn2 =
      build_conn()
      |> authed_conn(did, "/api/keyrings")
      |> get("/api/keyrings?limit=2&cursor=#{URI.encode_www_form(response1["cursor"])}")

    response2 = json_response(conn2, 200)
    assert length(response2["keyrings"]) == 1
  end
end
