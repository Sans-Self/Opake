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

  @workspace_id "at://did:plc:alice/app.opake.keyring/genesis"
  @editor_did "did:plc:bob"
  @viewer_did "did:plc:carol"
  @keyring_uri "at://did:plc:alice/app.opake.keyring/head"

  @f1 "at://did:plc:alice/app.opake.document/f1"
  @keep "at://did:plc:alice/app.opake.document/keep"
  @f2 "at://did:plc:bob/app.opake.document/f2"
  @d1 "at://did:plc:alice/app.opake.directory/d1"

  defp now, do: DateTime.utc_now()

  defp put_record(attrs) do
    {:ok, _} =
      RecordQueries.upsert(
        Map.merge(%{author_did: "did:plc:alice", cid: "bafy#{attrs.uri}", indexed_at: now()}, attrs)
      )
  end

  defp entry(target), do: %{"target" => target, "targetCid" => %{"$link" => "bafy#{target}"}}

  setup do
    # A keyring head where Bob is an editor, Carol a viewer.
    put_record(%{
      uri: @keyring_uri,
      collection: "app.opake.keyring",
      workspace_id: @workspace_id,
      record_jsonb: %{
        "members" => [
          %{"wrappedKey" => %{"did" => "did:plc:alice"}, "role" => "manager"},
          %{"wrappedKey" => %{"did" => @editor_did}, "role" => "editor"},
          %{"wrappedKey" => %{"did" => @viewer_did}, "role" => "viewer"}
        ]
      }
    })

    {:ok, _} = ChainHeadQueries.create(@workspace_id, "keyring", @keyring_uri, "bafykeyring")

    # Prior canonical directory: [f1, keep].
    put_record(%{
      uri: @d1,
      collection: "app.opake.directory",
      workspace_id: @workspace_id,
      record_jsonb: %{"entries" => [entry(@f1), entry(@keep)]}
    })

    :ok
  end

  defp seed_f2(supersedes) do
    put_record(%{
      uri: @f2,
      collection: "app.opake.document",
      author_did: @editor_did,
      workspace_id: @workspace_id,
      supersedes_uri: supersedes,
      record_jsonb: if(supersedes, do: %{"supersedes" => supersedes}, else: %{})
    })
  end

  describe "check_directory_supersede/4 — editor" do
    test "pure add passes (own contribution, nothing dropped)" do
      own = "at://did:plc:bob/app.opake.document/own"

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f1), entry(@keep), entry(own)]
             ) == :ok
    end

    test "advance passes when the substitute supersedes the dropped entry" do
      seed_f2(@f1)

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f2), entry(@keep)]
             ) == :ok
    end

    test "bare delete is rejected" do
      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@keep)]
             ) == {:rejected, :additivity_violation}
    end

    test "substitute that supersedes nothing is rejected (disguised delete)" do
      seed_f2(nil)

      assert Authority.check_directory_supersede(
               @workspace_id,
               @d1,
               @editor_did,
               [entry(@f2), entry(@keep)]
             ) == {:rejected, :additivity_violation}
    end

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
    test "manager faces no additivity constraint (bare delete allowed)" do
      put_record(%{
        uri: @keyring_uri,
        collection: "app.opake.keyring",
        workspace_id: @workspace_id,
        record_jsonb: %{
          "members" => [%{"wrappedKey" => %{"did" => @editor_did}, "role" => "manager"}]
        }
      })

      assert Authority.check_directory_supersede(@workspace_id, @d1, @editor_did, [entry(@keep)]) ==
               :ok
    end

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
end
