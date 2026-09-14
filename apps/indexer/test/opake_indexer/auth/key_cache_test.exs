defmodule OpakeIndexer.Auth.KeyCacheTest do
  use ExUnit.Case, async: false

  import ExUnit.CaptureLog

  alias OpakeIndexer.Auth.KeyCache

  defmodule Fetcher do
    @behaviour OpakeIndexer.Auth.KeyFetcherBehaviour

    # Blocks until the test releases it, so "one slow DID does not serialize
    # another" is decided by a handshake rather than by racing two sleeps.
    defp fetch_signing_key("did:plc:slow") do
      send(test_pid(), {:slow_started, self()})

      receive do
        :release -> {:ok, :binary.copy(<<1>>, 32)}
      after
        5_000 -> {:error, {:unavailable, :test_never_released}}
      end
    end

    defp fetch_signing_key("did:plc:state") do
      case :persistent_term.get({__MODULE__, :state}, :first) do
        :first -> {:ok, :binary.copy(<<2>>, 32)}
        :refused -> {:error, {:invalid, :account_public_key_signature}}
      end
    end

    defp fetch_signing_key("did:plc:healthy"), do: {:ok, :binary.copy(<<3>>, 32)}
    defp fetch_signing_key("did:plc:unverified"), do: {:ok, :binary.copy(<<4>>, 32)}
    defp fetch_signing_key("did:web:verified"), do: {:ok, :binary.copy(<<5>>, 32)}
    defp fetch_signing_key("did:plc:bulk" <> _), do: {:ok, :binary.copy(<<6>>, 32)}

    defp test_pid, do: :persistent_term.get({__MODULE__, :test_pid})

    @impl true
    def fetch_authentication_decision("did:plc:unverified") do
      {:ok, %{key: :binary.copy(<<4>>, 32), verified: false, anchor_history: :no_history}}
    end

    def fetch_authentication_decision("did:web:verified") do
      {:ok, %{key: :binary.copy(<<5>>, 32), verified: true, anchor_history: :no_history}}
    end

    def fetch_authentication_decision("did:plc:replaced") do
      {:ok, %{key: :binary.copy(<<7>>, 32), verified: true, anchor_history: :replaced}}
    end

    def fetch_authentication_decision("did:plc:historyless") do
      {:ok, %{key: :binary.copy(<<8>>, 32), verified: true, anchor_history: :unavailable}}
    end

    # Fails once with a transport error, then resolves.
    def fetch_authentication_decision("did:plc:flapping") do
      case :persistent_term.get({__MODULE__, :flapping}, :down) do
        :down ->
          :persistent_term.put({__MODULE__, :flapping}, :up)
          {:error, {:unavailable, :plc_directory}}

        :up ->
          {:ok, %{key: :binary.copy(<<9>>, 32), verified: true, anchor_history: :not_replaced}}
      end
    end

    # Refuses once, then would resolve — a second refusal proves the refusal
    # was served from the cache rather than re-fetched.
    def fetch_authentication_decision("did:plc:refused-once") do
      case :persistent_term.get({__MODULE__, :refused_once}, :refusing) do
        :refusing ->
          :persistent_term.put({__MODULE__, :refused_once}, :resolving)
          {:error, {:invalid, :account_public_key_signature}}

        :resolving ->
          {:ok, %{key: :binary.copy(<<10>>, 32), verified: true, anchor_history: :not_replaced}}
      end
    end

    def fetch_authentication_decision(did) do
      with {:ok, key} <- fetch_signing_key(did),
           do: {:ok, %{key: key, verified: true, anchor_history: :not_replaced}}
    end
  end

  setup do
    old_fetcher = Application.get_env(:opake_indexer, :key_fetcher)
    old_ttl = Application.get_env(:opake_indexer, :key_cache_ttl_ms)
    old_max = Application.get_env(:opake_indexer, :key_cache_max_entries)
    old_clock = Application.get_env(:opake_indexer, :key_cache_clock)
    Application.put_env(:opake_indexer, :key_fetcher, Fetcher)
    :ets.delete_all_objects(:key_cache)
    :persistent_term.put({Fetcher, :state}, :first)
    :persistent_term.put({Fetcher, :test_pid}, self())

    on_exit(fn ->
      restore(:key_fetcher, old_fetcher)
      restore(:key_cache_ttl_ms, old_ttl)
      restore(:key_cache_max_entries, old_max)
      restore(:key_cache_clock, old_clock)
      :persistent_term.erase({Fetcher, :state})
      :persistent_term.erase({Fetcher, :test_pid})
      :persistent_term.erase({Fetcher, :flapping})
      :persistent_term.erase({Fetcher, :refused_once})
      :ets.delete_all_objects(:key_cache)
    end)

    :ok
  end

  test "a slow DID lookup does not serialize a healthy DID" do
    slow = Task.async(fn -> KeyCache.get_key("did:plc:slow") end)
    assert_receive {:slow_started, slow_fetch}, 5_000
    assert {:ok, _} = KeyCache.get_key("did:plc:healthy")
    send(slow_fetch, :release)
    assert {:ok, _} = Task.await(slow)
  end

  test "an expired decision is re-resolved and can change to refusal" do
    freeze_clock(0)
    Application.put_env(:opake_indexer, :key_cache_ttl_ms, 10)
    assert {:ok, _} = KeyCache.get_key("did:plc:state")
    :persistent_term.put({Fetcher, :state}, :refused)
    advance_clock(9)
    assert {:ok, _} = KeyCache.get_key("did:plc:state")
    advance_clock(2)
    assert {:error, {:invalid, :account_public_key_signature}} = KeyCache.get_key("did:plc:state")
  end

  test "the cache retains key, verification state, and history notice as one decision" do
    assert {:ok, %{key: key, verified: true, anchor_history: :not_replaced}} =
             KeyCache.get_decision("did:plc:healthy")

    assert {:ok, ^key} = KeyCache.get_key("did:plc:healthy")
  end

  test "an absent history notice does not conflate unverified and verified did:web decisions" do
    assert {:ok, %{verified: false, anchor_history: :no_history}} =
             KeyCache.get_decision("did:plc:unverified")

    assert {:ok, %{verified: true, anchor_history: :no_history}} =
             KeyCache.get_decision("did:web:verified")
  end

  test "a transport failure is not cached, so recovery is served immediately" do
    assert {:error, {:unavailable, :plc_directory}} = KeyCache.get_decision("did:plc:flapping")
    assert :ets.lookup(:key_cache, "did:plc:flapping") == []
    assert {:ok, %{verified: true}} = KeyCache.get_decision("did:plc:flapping")
  end

  test "a refusal is cached for the decision lifetime" do
    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyCache.get_decision("did:plc:refused-once")

    assert {:error, {:invalid, :account_public_key_signature}} =
             KeyCache.get_decision("did:plc:refused-once")
  end

  test "expired entries are not swept on every miss" do
    freeze_clock(0)
    Application.put_env(:opake_indexer, :key_cache_ttl_ms, 10)
    assert {:ok, _} = KeyCache.get_key("did:plc:healthy")
    advance_clock(50)
    assert {:ok, _} = KeyCache.get_key("did:plc:unverified")
    assert [{"did:plc:healthy", _, _, _}] = :ets.lookup(:key_cache, "did:plc:healthy")
  end

  test "expired entries are reclaimed by the amortised sweep" do
    freeze_clock(0)
    Application.put_env(:opake_indexer, :key_cache_ttl_ms, 10)
    Enum.each(1..300, &KeyCache.get_key("did:plc:bulk#{&1}"))
    advance_clock(50)
    Enum.each(301..600, &KeyCache.get_key("did:plc:bulk#{&1}"))
    assert :ets.info(:key_cache, :size) < 600
  end

  test "eviction at capacity drops the oldest entry" do
    freeze_clock(0)
    Application.put_env(:opake_indexer, :key_cache_max_entries, 3)

    Enum.each(1..3, fn n ->
      advance_clock(1)
      assert {:ok, _} = KeyCache.get_key("did:plc:bulk#{n}")
    end)

    advance_clock(1)
    assert {:ok, _} = KeyCache.get_key("did:plc:bulk4")
    assert :ets.lookup(:key_cache, "did:plc:bulk1") == []
    assert [_] = :ets.lookup(:key_cache, "did:plc:bulk2")
    assert [_] = :ets.lookup(:key_cache, "did:plc:bulk4")
  end

  test "a replaced anchor is reported to operators on the resolution path" do
    log =
      capture_log(fn ->
        assert {:ok, %{verified: true}} = KeyCache.get_decision("did:plc:replaced")
      end)

    assert log =~ "did:plc:replaced"
    assert log =~ "replaced"
    assert log =~ "[warning]"
  end

  test "an unreadable anchor history is reported without raising the severity" do
    log =
      capture_at_info(fn ->
        assert {:ok, %{verified: true}} = KeyCache.get_decision("did:plc:historyless")
      end)

    assert log =~ "did:plc:historyless"
    assert log =~ "[info]"
  end

  test "a clean verified decision logs no anchor notice" do
    log =
      capture_at_info(fn ->
        assert {:ok, _} = KeyCache.get_decision("did:plc:healthy")
      end)

    refute log =~ "anchor"
  end

  # The suite runs at :warning; the informational notice needs the level lowered
  # for the duration of the capture.
  defp capture_at_info(fun) do
    previous = Logger.level()
    Logger.configure(level: :info)

    try do
      capture_log(fun)
    after
      Logger.configure(level: previous)
    end
  end

  defp freeze_clock(at) do
    :persistent_term.put({__MODULE__, :now}, at)
    Application.put_env(:opake_indexer, :key_cache_clock, fn -> fake_now() end)
  end

  defp advance_clock(by), do: :persistent_term.put({__MODULE__, :now}, fake_now() + by)

  defp fake_now, do: :persistent_term.get({__MODULE__, :now})

  defp restore(key, nil), do: Application.delete_env(:opake_indexer, key)
  defp restore(key, value), do: Application.put_env(:opake_indexer, key, value)
end
