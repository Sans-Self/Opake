defmodule OpakeIndexer.PipelineTest do
  @moduledoc """
  End-to-end pipeline tests: raw Jetstream JSON → Firehose.process_message → DB → query results.
  """

  use OpakeIndexer.DataCase, async: true

  import Ecto.Query

  alias OpakeIndexer.Firehose

  alias OpakeIndexer.Queries.{
    DirectoryQueries,
    DocumentQueries,
    DocumentUpdateQueries,
    KeyringQueries
  }

  # -- Helpers --

  defp document_create_json(did, rkey, encryption) do
    Jason.encode!(%{
      "did" => did,
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.document",
        "rkey" => rkey,
        "record" => %{
          "encryption" => encryption
        }
      }
    })
  end

  defp document_update_create_json(editor_did, rkey, document_uri) do
    Jason.encode!(%{
      "did" => editor_did,
      "time_us" => 1_709_330_500_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.documentUpdate",
        "rkey" => rkey,
        "record" => %{
          "document" => document_uri
        }
      }
    })
  end

  defp keyring_leave_json(member_did, keyring_uri) do
    Jason.encode!(%{
      "did" => member_did,
      "time_us" => 1_709_330_600_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.keyringLeave",
        "rkey" => "3leave",
        "record" => %{
          "keyring" => keyring_uri
        }
      }
    })
  end

  defp keyring_create_json_with_roles(owner_did, rkey, members, opts \\ []) do
    rotation = Keyword.get(opts, :rotation, 0)
    encrypted_metadata = Keyword.get(opts, :encrypted_metadata)
    created_at = Keyword.get(opts, :created_at, "2026-03-01T12:00:00Z")

    record =
      %{
        "members" =>
          Enum.map(members, fn {did, role} ->
            %{
              "wrappedKey" => %{
                "did" => did,
                "ciphertext" => %{"$bytes" => "AAAA"},
                "algo" => "x25519-mlkem768-hkdf-a256kw-v2"
              },
              "role" => role
            }
          end),
        "rotation" => rotation,
        "createdAt" => created_at
      }
      |> then(fn r ->
        if encrypted_metadata, do: Map.put(r, "encryptedMetadata", encrypted_metadata), else: r
      end)

    Jason.encode!(%{
      "did" => owner_did,
      "time_us" => 1_709_330_400_000_000,
      "kind" => "commit",
      "commit" => %{
        "rev" => "abc",
        "operation" => "create",
        "collection" => "app.opake.keyring",
        "rkey" => rkey,
        "record" => record
      }
    })
  end

  # -- Tests --

  test "document with keyringEncryption indexes with keyring_uri" do
    keyring_uri = "at://did:plc:owner/app.opake.keyring/3kr"

    json =
      document_create_json("did:plc:alice", "3doc", %{
        "keyringRef" => %{"keyring" => keyring_uri, "rotation" => 0}
      })

    Firehose.process_message(json, 0)

    docs = DocumentQueries.list_documents(keyring_uri)
    assert length(docs) == 1

    doc = hd(docs)
    assert doc.document_uri == "at://did:plc:alice/app.opake.document/3doc"
    assert doc.keyring_uri == keyring_uri
    assert doc.owner_did == "did:plc:alice"
    assert doc.rotation == 0
  end

  test "document with directEncryption indexes with nil keyring_uri" do
    json =
      document_create_json("did:plc:alice", "3doc", %{
        "envelope" => %{"algo" => "aes-256-gcm"}
      })

    Firehose.process_message(json, 0)

    # Should be in the cabinet tree (no keyring)
    {dirs, docs} = DirectoryQueries.cabinet_tree("did:plc:alice")
    assert length(docs) == 1
    assert hd(docs).keyring_uri == nil
  end

  test "documentUpdate create flows through to document_updates" do
    document_uri = "at://did:plc:owner/app.opake.document/3xyz"

    json = document_update_create_json("did:plc:editor", "3upd", document_uri)
    Firehose.process_message(json, 0)

    {updates, _cursor} = DocumentUpdateQueries.list_document_updates(document_uri)
    assert length(updates) == 1

    update = hd(updates)
    assert update.uri == "at://did:plc:editor/app.opake.documentUpdate/3upd"
    assert update.document_uri == document_uri
    assert update.author_did == "did:plc:editor"
  end

  test "documentUpdate on a workspace document broadcasts to the keyring topic" do
    # Seed a workspace document so the indexer's keyring lookup succeeds
    # when the documentUpdate event arrives. The JOIN through the documents
    # table is what routes this event to workspace members (the lexicon
    # itself has no `keyring` reference).
    keyring_uri = "at://did:plc:docupdate_routed/app.opake.keyring/3kr"
    document_uri = "at://did:plc:docupdate_routed/app.opake.document/3xyz"

    {:ok, _doc} =
      DocumentQueries.upsert_document(%{
        document_uri: document_uri,
        keyring_uri: keyring_uri,
        owner_did: "did:plc:docupdate_routed",
        indexed_at: DateTime.utc_now()
      })

    Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, OpakeIndexer.SSE.Topics.workspace(keyring_uri))

    json = document_update_create_json("did:plc:docupdate_editor", "3upd_routed", document_uri)
    Firehose.process_message(json, 0)

    assert_receive {:sse_event, "document_update:upsert", payload}
    assert payload.document_uri == document_uri
    assert payload.keyring_uri == keyring_uri
    assert payload.author_did == "did:plc:docupdate_editor"
  end

  test "documentUpdate for an unknown document is dropped (no broadcast)" do
    # The proposal arrives without a resolvable parent document — either
    # the document belongs to the cabinet (no workspace to route to) or
    # it hasn't been indexed yet (backfill ordering edge case). The
    # broadcaster drops it silently. The DB row IS still written, so the
    # owner's next `sync_workspace_by_uri` call will pick it up from the
    # persistent proposal store whenever it runs.
    document_uri = "at://did:plc:docupdate_unknown_owner/app.opake.document/3xyz"
    author_did = "did:plc:docupdate_unknown_editor"

    # Subscribe to both topics that could plausibly carry the event. We
    # expect NEITHER to fire.
    Phoenix.PubSub.subscribe(OpakeIndexer.PubSub, OpakeIndexer.SSE.Topics.personal(author_did))

    Phoenix.PubSub.subscribe(
      OpakeIndexer.PubSub,
      OpakeIndexer.SSE.Topics.workspace("at://did:plc:docupdate_unknown_owner/app.opake.keyring/3kr")
    )

    json = document_update_create_json(author_did, "3upd_unknown", document_uri)
    Firehose.process_message(json, 0)

    refute_receive {:sse_event, "document_update:upsert", _}, 50

    # And the DB row was written — that's the backstop.
    {updates, _} = DocumentUpdateQueries.list_document_updates(document_uri)
    assert length(updates) == 1
  end

  test "keyringLeave removes the leaving member" do
    keyring_uri = "at://did:plc:owner/app.opake.keyring/3kr"

    {:ok, _} =
      KeyringQueries.upsert_keyring(keyring_uri, "did:plc:owner", [
        %{did: "did:plc:alice", role: "manager"},
        %{did: "did:plc:bob", role: "editor"}
      ])

    assert KeyringQueries.is_member?(keyring_uri, "did:plc:bob")

    json = keyring_leave_json("did:plc:bob", keyring_uri)
    Firehose.process_message(json, 0)

    refute KeyringQueries.is_member?(keyring_uri, "did:plc:bob")
    assert KeyringQueries.is_member?(keyring_uri, "did:plc:alice")
  end

  test "keyring create with roles indexes members correctly" do
    owner = "did:plc:owner"
    rkey = "3kr"
    keyring_uri = "at://#{owner}/app.opake.keyring/#{rkey}"

    json =
      keyring_create_json_with_roles(owner, rkey, [
        {"did:plc:alice", "manager"},
        {"did:plc:bob", "editor"},
        {"did:plc:charlie", "viewer"}
      ])

    Firehose.process_message(json, 0)

    assert KeyringQueries.is_member?(keyring_uri, "did:plc:alice")
    assert KeyringQueries.is_member?(keyring_uri, "did:plc:bob")
    assert KeyringQueries.is_member?(keyring_uri, "did:plc:charlie")

    members =
      Repo.all(
        from(km in OpakeIndexer.Schemas.KeyringMember,
          where: km.keyring_uri == ^keyring_uri,
          order_by: km.member_did
        )
      )

    assert length(members) == 3

    roles = Map.new(members, &{&1.member_did, &1.role})
    assert roles["did:plc:alice"] == "manager"
    assert roles["did:plc:bob"] == "editor"
    assert roles["did:plc:charlie"] == "viewer"
  end

  test "keyring create indexes full record into keyrings table" do
    owner = "did:plc:owner"
    rkey = "3krfull"
    keyring_uri = "at://#{owner}/app.opake.keyring/#{rkey}"
    enc_meta = %{"ciphertext" => "AAAA", "nonce" => "BBBB"}

    json =
      keyring_create_json_with_roles(
        owner,
        rkey,
        [
          {"did:plc:alice", "manager"},
          {"did:plc:bob", "editor"}
        ],
        rotation: 3,
        encrypted_metadata: enc_meta,
        created_at: "2026-03-10T08:00:00Z"
      )

    Firehose.process_message(json, 0)

    keyring = Repo.get(OpakeIndexer.Schemas.Keyring, keyring_uri)
    assert keyring != nil
    assert keyring.owner_did == owner
    assert keyring.rotation == 3
    assert keyring.encrypted_metadata == enc_meta
    assert keyring.created_at == "2026-03-10T08:00:00Z"

    # Members stored in keyring_members with wrapped keys
    members =
      from(km in OpakeIndexer.Schemas.KeyringMember, where: km.keyring_uri == ^keyring_uri)
      |> Repo.all()

    assert length(members) == 2
    assert Enum.any?(members, &(&1.wrapped_key["did"] == "did:plc:alice"))
  end

  test "keyring delete removes from both keyrings and keyring_members" do
    owner = "did:plc:owner"
    rkey = "3krdel"
    keyring_uri = "at://#{owner}/app.opake.keyring/#{rkey}"

    create_json =
      keyring_create_json_with_roles(owner, rkey, [{"did:plc:alice", "manager"}])

    Firehose.process_message(create_json, 0)

    assert KeyringQueries.is_member?(keyring_uri, "did:plc:alice")
    assert Repo.get(OpakeIndexer.Schemas.Keyring, keyring_uri) != nil

    delete_json =
      Jason.encode!(%{
        "did" => owner,
        "time_us" => 1_709_330_500_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "delete",
          "collection" => "app.opake.keyring",
          "rkey" => rkey
        }
      })

    Firehose.process_message(delete_json, 1)

    refute KeyringQueries.is_member?(keyring_uri, "did:plc:alice")
    assert Repo.get(OpakeIndexer.Schemas.Keyring, keyring_uri) == nil
  end

  test "directory with keyringKeyWrapping indexes with keyring_uri" do
    keyring_uri = "at://did:plc:owner/app.opake.keyring/3kr"

    json =
      Jason.encode!(%{
        "did" => "did:plc:alice",
        "time_us" => 1_709_330_400_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "create",
          "collection" => "app.opake.directory",
          "rkey" => "3dir",
          "record" => %{
            "keyWrapping" => %{
              "$type" => "app.opake.defs#keyringKeyWrapping",
              "keyringRef" => %{"keyring" => keyring_uri}
            },
            "entries" => ["at://did:plc:alice/app.opake.document/3doc"]
          }
        }
      })

    Firehose.process_message(json, 0)

    {dirs, _docs} = DirectoryQueries.workspace_tree(keyring_uri)
    assert length(dirs) == 1

    dir = hd(dirs)
    assert dir.directory_uri == "at://did:plc:alice/app.opake.directory/3dir"
    assert dir.keyring_uri == keyring_uri
    assert dir.owner_did == "did:plc:alice"
    assert dir.entries == ["at://did:plc:alice/app.opake.document/3doc"]
  end

  test "directory with directKeyWrapping indexes with nil keyring_uri" do
    json =
      Jason.encode!(%{
        "did" => "did:plc:alice",
        "time_us" => 1_000_000,
        "kind" => "commit",
        "commit" => %{
          "rev" => "abc",
          "operation" => "create",
          "collection" => "app.opake.directory",
          "rkey" => "3dir",
          "record" => %{
            "keyWrapping" => %{
              "$type" => "app.opake.defs#directKeyWrapping",
              "envelope" => %{"algo" => "aes-256-gcm"}
            }
          }
        }
      })

    Firehose.process_message(json, 0)

    {dirs, _docs} = DirectoryQueries.cabinet_tree("did:plc:alice")
    assert length(dirs) == 1
    assert hd(dirs).keyring_uri == nil
  end

  # directoryUpdate indexing removed — the delta broker gets structure
  # changes from the directory record itself (entries array), not from
  # separate directoryUpdate records. Audit log can be re-added later.
end
