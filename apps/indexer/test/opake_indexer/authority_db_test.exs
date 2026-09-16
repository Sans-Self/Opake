defmodule OpakeIndexer.AuthorityDbTest do
  @moduledoc """
  DB-backed coverage for `check_directory_supersede/4` on the editor path.

  The pure additivity decision is unit-tested in `authority_test.exs`; this
  file pins down the resolution layer that reads roles from the live keyring
  and the `supersedes` field off the *added target's* record — the part that
  makes an editor's edit (advance) distinguishable from a disguised delete.
  """

  use OpakeIndexer.DataCase, async: true

  alias OpakeIndexer.Authority
  alias OpakeIndexer.Queries.{ChainHeadQueries, RecordQueries}

  @workspace_id "at://did:plc:alice/at.opake.keyring/genesis"
  @editor_did "did:plc:bob"
  @viewer_did "did:plc:carol"
  @keyring_uri "at://did:plc:alice/at.opake.keyring/head"

  @f1 "at://did:plc:alice/at.opake.document/f1"
  @keep "at://did:plc:alice/at.opake.document/keep"
  @f2 "at://did:plc:bob/at.opake.document/f2"
  @d1 "at://did:plc:alice/at.opake.directory/d1"

  defp now, do: DateTime.utc_now()

  defp put_record(attrs) do
    {:ok, _} =
      RecordQueries.upsert(
        Map.merge(
          %{author_did: "did:plc:alice", cid: "bafy#{attrs.uri}", indexed_at: now()},
          attrs
        )
      )
  end

  defp entry(target), do: %{"target" => target, "targetCid" => %{"$link" => "bafy#{target}"}}

  setup do
    # A keyring head where Bob is an editor, Carol a viewer.
    put_record(%{
      uri: @keyring_uri,
      collection: "at.opake.keyring",
      workspace_id: @workspace_id,
      record_jsonb:
        head_fields([
          %{"did" => "did:plc:alice", "role" => "manager"},
          %{"did" => @editor_did, "role" => "editor"},
          %{"did" => @viewer_did, "role" => "viewer"}
        ])
    })

    {:ok, _} = ChainHeadQueries.create(@workspace_id, "keyring", @keyring_uri, "bafykeyring")

    # Prior canonical directory: [f1, keep].
    put_record(%{
      uri: @d1,
      collection: "at.opake.directory",
      workspace_id: @workspace_id,
      record_jsonb: %{"entries" => [entry(@f1), entry(@keep)]}
    })

    :ok
  end

  defp seed_f2(supersedes) do
    put_record(%{
      uri: @f2,
      collection: "at.opake.document",
      author_did: @editor_did,
      workspace_id: @workspace_id,
      supersedes_uri: supersedes,
      record_jsonb: if(supersedes, do: %{"supersedes" => supersedes}, else: %{})
    })
  end

  describe "check_directory_supersede/4 — editor" do
    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "pure add passes (own contribution, nothing dropped)" do
      own = "at://did:plc:bob/at.opake.document/own"

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f1), entry(@keep), entry(own)]
             ) == :ok
    end

    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "advance passes when the substitute supersedes the dropped entry" do
      seed_f2(@f1)

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f2), entry(@keep)]
             ) == :ok
    end

    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "bare delete is rejected" do
      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@keep)]
             ) == {:rejected, :additivity_violation}
    end

    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "substitute that supersedes nothing is rejected (disguised delete)" do
      seed_f2(nil)

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f2), entry(@keep)]
             ) == {:rejected, :additivity_violation}
    end

    # spec:tree-chains § Cascades write leaf-first so the indexer resolves additivity in arrival order
    test "substitute not yet indexed is rejected (heals on reprocess)" do
      # f2 referenced as an entry but its record hasn't landed — no
      # supersede claim is resolvable, so the dropped f1 reads as a bare
      # delete until the doc arrives.
      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f2), entry(@keep)]
             ) == {:rejected, :additivity_violation}
    end
  end

  describe "check_directory_supersede/4 — role gates" do
    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "manager faces no additivity constraint (bare delete allowed)" do
      put_record(%{
        uri: @keyring_uri,
        collection: "at.opake.keyring",
        workspace_id: @workspace_id,
        record_jsonb: %{
          "members" => [%{"did" => @editor_did, "role" => "manager"}]
        }
      })

      assert Authority.check_directory_supersede(@workspace_id, @d1, @editor_did, [entry(@keep)]) ==
               :ok
    end

    # spec:tree-chains § Editor supersedes are additive; managers are unrestricted
    test "viewer cannot author a directory supersede" do
      assert Authority.check_directory_supersede(@workspace_id, @d1, @viewer_did, [
               entry(@f1),
               entry(@keep)
             ]) == {:rejected, :insufficient_role}
    end

    test "non-member is rejected" do
      assert Authority.check_directory_supersede(@workspace_id, @d1, "did:plc:stranger", [
               entry(@f1),
               entry(@keep)
             ]) == {:rejected, :not_a_member}
    end
  end

  # Head members are alice (manager), bob (editor), carol (viewer) — see setup.
  defp member(did, role), do: %{"did" => did, "role" => role}

  defp wrapped_member(did, role, ciphertext, approval) do
    %{
      "did" => did,
      "role" => role,
      "wrappedKey" => %{
        "did" => did,
        "algo" => "x25519-mlkem768-hkdf-a256kw-v2",
        "ciphertext" => %{"$bytes" => ciphertext}
      },
      "unverifiedKeyApproval" => %{"$bytes" => approval}
    }
  end

  # The head keyring as the Rust client actually writes it: a leave carries
  # every field of the prior record verbatim apart from the departing member.
  defp head_fields(members) do
    %{
      "opakeVersion" => 1,
      "algo" => "aes-256-gcm",
      "rotation" => 4,
      "keyHistory" => [%{"rotation" => 3, "members" => []}],
      "encryptedMetadata" => %{
        "ciphertext" => %{"$bytes" => "Y2lwaGVy"},
        "nonce" => %{"$bytes" => "bm9uY2U"}
      },
      "members" => members
    }
  end

  defp leave_record(members), do: head_fields(members)

  describe "check_keyring_supersede/4 — self-removal (leave)" do
    test "genesis (no prior) skips the check" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               nil,
               "did:plc:anyone",
               leave_record([])
             ) == :ok
    end

    test "manager supersede passes regardless of member changes" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               "did:plc:alice",
               leave_record([member("did:plc:alice", "manager")])
             ) == :ok
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "editor leaving passes: prior list minus exactly themselves" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([member("did:plc:alice", "manager"), member(@viewer_did, "viewer")])
             ) == :ok
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "viewer leaving passes" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @viewer_did,
               leave_record([member("did:plc:alice", "manager"), member(@editor_did, "editor")])
             ) == :ok
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "a complete leave record carrying every client-emitted field is accepted" do
      record =
        [member("did:plc:alice", "manager"), member(@viewer_did, "viewer")]
        |> leave_record()
        |> Map.merge(%{
          "supersedes" => @keyring_uri,
          "supersedesCid" => %{"$link" => "bafykeyring"},
          "lineage" => @workspace_id,
          "createdAt" => "2026-09-12T10:00:00Z"
        })

      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               record
             ) == :ok
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "editor dropping someone else alongside themselves is rejected" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([member("did:plc:alice", "manager")])
             ) == {:rejected, :insufficient_role}
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "editor re-roling a remaining member while leaving is rejected" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([member("did:plc:alice", "manager"), member(@viewer_did, "editor")])
             ) == {:rejected, :insufficient_role}
    end

    # spec:workspace-membership § Keyring supersede authority is manager-only, except pure self-removal
    test "editor adding a member while leaving is rejected" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([
                 member("did:plc:alice", "manager"),
                 member(@viewer_did, "viewer"),
                 member("did:plc:mallory", "editor")
               ])
             ) == {:rejected, :insufficient_role}
    end

    test "editor supersede that keeps themselves is rejected" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([
                 member("did:plc:alice", "manager"),
                 member(@editor_did, "editor"),
                 member(@viewer_did, "viewer")
               ])
             ) == {:rejected, :insufficient_role}
    end

    test "self-removal cannot replace a remaining member's wrap ciphertext" do
      approval = Base.encode64(:binary.copy(<<7>>, 32))

      put_record(%{
        uri: @keyring_uri,
        collection: "at.opake.keyring",
        workspace_id: @workspace_id,
        record_jsonb:
          head_fields([
            wrapped_member("did:plc:alice", "manager", Base.encode64(<<1>>), approval),
            member(@editor_did, "editor"),
            member(@viewer_did, "viewer")
          ])
      })

      # Rewrapping needs manager authority. A leave carries each remaining
      # member's actual wrapped bytes verbatim (apart from base64 rendering).
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([
                 wrapped_member(
                   "did:plc:alice",
                   "manager",
                   Base.encode64(<<2>>),
                   String.trim_trailing(approval, "=")
                 ),
                 member(@viewer_did, "viewer")
               ])
             ) == {:rejected, :insufficient_role}
    end

    test "self-removal cannot poison rotation, history, metadata, or future fields" do
      prior =
        [
          member("did:plc:alice", "manager"),
          member(@editor_did, "editor"),
          member(@viewer_did, "viewer")
        ]
        |> head_fields()
        |> Map.put("futureSecurityField", %{"nested" => true})

      put_record(%{
        uri: @keyring_uri,
        collection: "at.opake.keyring",
        workspace_id: @workspace_id,
        record_jsonb: prior
      })

      poisoned =
        prior
        |> Map.put("rotation", 9_999)
        |> Map.put("keyHistory", [])
        |> Map.put("encryptedMetadata", %{
          "ciphertext" => %{"$bytes" => "cG9pc29u"},
          "nonce" => %{"$bytes" => "bm9uY2U"}
        })
        |> Map.put("futureSecurityField", %{"nested" => false})
        |> Map.put("members", [member("did:plc:alice", "manager"), member(@viewer_did, "viewer")])
        |> Map.put("supersedes", @keyring_uri)

      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               poisoned
             ) == {:rejected, :insufficient_role}
    end

    test "self-removal cannot introduce a wrap for a remaining member" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               @editor_did,
               leave_record([
                 %{
                   "did" => "did:plc:alice",
                   "role" => "manager",
                   "wrappedKey" => %{"did" => "did:plc:alice"}
                 },
                 member(@viewer_did, "viewer")
               ])
             ) == {:rejected, :insufficient_role}
    end

    test "non-member is rejected" do
      assert Authority.check_keyring_supersede(
               @workspace_id,
               @keyring_uri,
               "did:plc:stranger",
               leave_record([member("did:plc:alice", "manager")])
             ) == {:rejected, :not_a_member}
    end
  end
end
