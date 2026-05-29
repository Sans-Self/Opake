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

  describe "check_workspace_root_flag/2" do
    # Pure function, no DB — safe to exercise here.

    test "no prior record always passes" do
      assert Authority.check_workspace_root_flag(nil, true) == :ok
      assert Authority.check_workspace_root_flag(nil, false) == :ok
    end

    test "flag unchanged passes" do
      prior = %{record_jsonb: %{"isWorkspaceRoot" => true}}
      assert Authority.check_workspace_root_flag(prior, true) == :ok
    end

    test "absent prior flag matches new false" do
      prior = %{record_jsonb: %{"otherField" => 1}}
      assert Authority.check_workspace_root_flag(prior, false) == :ok
    end

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
