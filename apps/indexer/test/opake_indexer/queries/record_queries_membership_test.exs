defmodule OpakeIndexer.Queries.RecordQueriesMembershipTest do
  @moduledoc """
  The three-valued membership resolution the workspace endpoints map to the
  wire: no chain head, head without the caller, and member with a role.
  """

  use OpakeIndexer.DataCase, async: true

  alias OpakeIndexer.Queries.{ChainHeadQueries, RecordQueries}

  @owner "did:plc:owner"
  @outsider "did:plc:outsider"
  @genesis "at://#{@owner}/at.opake.keyring/genesis"

  defp put_keyring(uri, members) do
    {:ok, record} =
      RecordQueries.upsert(%{
        uri: uri,
        collection: "at.opake.keyring",
        author_did: @owner,
        workspace_id: uri,
        cid: "bafy-#{uri}",
        indexed_at: DateTime.utc_now(),
        record_jsonb: %{
          "members" =>
            Enum.map(members, fn {did, role} ->
              %{"did" => did, "role" => role}
            end)
        }
      })

    {:ok, _} = ChainHeadQueries.create(uri, "keyring", uri, record.cid)
    record
  end

  # spec:indexer-consistency § Unknown workspace is distinguishable from non-membership
  test "no chain head resolves to :workspace_not_indexed" do
    assert RecordQueries.resolve_membership(@genesis, @owner) == :workspace_not_indexed
  end

  # spec:indexer-consistency § Unknown workspace is distinguishable from non-membership
  test "a head without the caller resolves to :not_a_member" do
    put_keyring(@genesis, [{@owner, "manager"}])

    assert RecordQueries.resolve_membership(@genesis, @outsider) == :not_a_member
  end

  # spec:indexer-consistency § Unknown workspace is distinguishable from non-membership
  test "a listed member resolves to its role" do
    put_keyring(@genesis, [{@owner, "manager"}, {@outsider, "editor"}])

    assert RecordQueries.resolve_membership(@genesis, @owner) == {:member, "manager"}
    assert RecordQueries.resolve_membership(@genesis, @outsider) == {:member, "editor"}
  end

  # spec:indexer-consistency § Unknown workspace is distinguishable from non-membership
  test "a torn-down chain reverts to :workspace_not_indexed for a former member" do
    put_keyring(@genesis, [{@owner, "manager"}])
    assert {:member, _} = RecordQueries.resolve_membership(@genesis, @owner)

    {1, nil} = RecordQueries.soft_delete(@genesis, DateTime.utc_now())
    ChainHeadQueries.delete_all(@genesis)

    assert RecordQueries.resolve_membership(@genesis, @owner) == :workspace_not_indexed
  end
end
