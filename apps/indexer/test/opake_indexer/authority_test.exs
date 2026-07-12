defmodule OpakeIndexer.AuthorityTest do
  @moduledoc """
  Coverage for the role-classification helper. The full
  `check_keyring_supersede/3` and `check_directory_supersede/4` paths
  hit the database and are exercised by the controller pipeline tests;
  this file pins down the in-memory parts that don't need a repo.

  The key behavior here is the unknown-role branch: any role string not
  in `@known_roles` must log a `Logger.warning` before being mapped to
  `:unknown`, so schema drift or corrupted records show up in operations
  instead of silently rejecting at the catch-all clause.
  """

  use ExUnit.Case, async: true

  import ExUnit.CaptureLog

  alias OpakeIndexer.Authority

  describe "classify_role/2 — known roles" do
    test "maps \"manager\" to :manager" do
      assert Authority.classify_role("manager") == :manager
    end

    test "maps \"editor\" to :editor" do
      assert Authority.classify_role("editor") == :editor
    end

    test "maps \"viewer\" to :viewer" do
      assert Authority.classify_role("viewer") == :viewer
    end

    test "known roles don't log" do
      log =
        capture_log(fn ->
          Authority.classify_role("manager", workspace_id: "ws-1")
          Authority.classify_role("editor", workspace_id: "ws-1")
          Authority.classify_role("viewer", workspace_id: "ws-1")
        end)

      refute log =~ "[Authority]"
    end
  end

  describe "classify_role/2 — missing membership" do
    test "maps nil to :missing" do
      assert Authority.classify_role(nil) == :missing
    end

    test "nil doesn't log — not a member is an expected case" do
      log = capture_log(fn -> Authority.classify_role(nil, workspace_id: "ws-1") end)
      refute log =~ "[Authority]"
    end
  end

  describe "classify_role/2 — anomalous data" do
    test "unknown string role maps to :unknown" do
      capture_log(fn ->
        assert Authority.classify_role("super-admin") == :unknown
      end)
    end

    test "unknown string role logs a warning with the role + workspace" do
      log =
        capture_log(fn ->
          Authority.classify_role("super-admin", workspace_id: "ws-42")
        end)

      assert log =~ "[Authority]"
      assert log =~ "super-admin"
      assert log =~ "ws-42"
    end

    test "non-string role maps to :unknown and logs" do
      log =
        capture_log(fn ->
          assert Authority.classify_role(%{"unexpected" => "shape"}, workspace_id: "ws-3") ==
                   :unknown
        end)

      assert log =~ "[Authority]"
      assert log =~ "non-string role"
      assert log =~ "ws-3"
    end

    test "missing workspace_id is logged as <unknown>" do
      log = capture_log(fn -> Authority.classify_role("bogus") end)
      assert log =~ "<unknown>"
    end
  end

  describe "additive?/3 — supersede-aware editor additivity" do
    # Pure decision, no DB. The DB-resolution wrapper (additivity_check/2)
    # is exercised by the firehose pipeline tests.

    defp targets(uris), do: MapSet.new(uris)

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "pure add passes — nothing dropped" do
      prior = targets(["at://a/doc/1"])
      new = targets(["at://a/doc/1", "at://b/doc/2"])
      assert Authority.additive?(prior, new, targets([])) == :ok
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "reorder passes — same target set" do
      prior = targets(["at://a/doc/1", "at://a/doc/2"])
      new = targets(["at://a/doc/2", "at://a/doc/1"])
      assert Authority.additive?(prior, new, targets([])) == :ok
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "advance passes — dropped entry is superseded by an added one" do
      # f1 replaced by f2, where f2.supersedes == f1.
      prior = targets(["at://a/doc/f1", "at://a/doc/keep"])
      new = targets(["at://b/doc/f2", "at://a/doc/keep"])
      claimed = targets(["at://a/doc/f1"])
      assert Authority.additive?(prior, new, claimed) == :ok
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "bare delete rejected — dropped entry with no superseding replacement" do
      prior = targets(["at://a/doc/f1", "at://a/doc/keep"])
      new = targets(["at://a/doc/keep"])
      assert Authority.additive?(prior, new, targets([])) ==
               {:rejected, :additivity_violation}
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "disguised delete rejected — substitute that supersedes the wrong entry" do
      # f1 dropped, g2 added, but g2 supersedes some unrelated h — f1 is
      # uncovered, so this is a delete wearing an edit's clothes.
      prior = targets(["at://a/doc/f1"])
      new = targets(["at://b/doc/g2"])
      claimed = targets(["at://x/doc/h"])
      assert Authority.additive?(prior, new, claimed) ==
               {:rejected, :additivity_violation}
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "partial cover rejected — one of two drops is superseded" do
      prior = targets(["at://a/doc/f1", "at://a/doc/f3"])
      new = targets(["at://b/doc/f2"])
      claimed = targets(["at://a/doc/f1"])
      assert Authority.additive?(prior, new, claimed) ==
               {:rejected, :additivity_violation}
    end

    # spec:directory-chains § Editor supersedes are additive; managers are unrestricted
    test "advance plus add passes — edit one entry and contribute another" do
      prior = targets(["at://a/doc/f1"])
      new = targets(["at://b/doc/f2", "at://b/doc/own"])
      claimed = targets(["at://a/doc/f1"])
      assert Authority.additive?(prior, new, claimed) == :ok
    end
  end

  describe "check_workspace_root_flag/2" do
    # Pure function, no DB — safe to exercise here.

    # spec:directory-chains § The workspace root is a flag-marked chain, forward-walked from genesis
    test "no prior record always passes" do
      assert Authority.check_workspace_root_flag(nil, true) == :ok
      assert Authority.check_workspace_root_flag(nil, false) == :ok
    end

    # spec:directory-chains § The workspace root is a flag-marked chain, forward-walked from genesis
    test "flag unchanged passes" do
      prior = %{record_jsonb: %{"isWorkspaceRoot" => true}}
      assert Authority.check_workspace_root_flag(prior, true) == :ok
    end

    # spec:directory-chains § The workspace root is a flag-marked chain, forward-walked from genesis
    test "absent prior flag matches new false" do
      prior = %{record_jsonb: %{"otherField" => 1}}
      assert Authority.check_workspace_root_flag(prior, false) == :ok
    end

    # spec:directory-chains § The workspace root is a flag-marked chain, forward-walked from genesis
    test "flag flipping is rejected" do
      true_prior = %{record_jsonb: %{"isWorkspaceRoot" => true}}
      false_prior = %{record_jsonb: %{"isWorkspaceRoot" => false}}

      assert Authority.check_workspace_root_flag(true_prior, false) ==
               {:rejected, :workspace_root_flip}

      assert Authority.check_workspace_root_flag(false_prior, true) ==
               {:rejected, :workspace_root_flip}
    end
  end
end
