defmodule OpakeAppview.SSE.TokenStoreTest do
  use ExUnit.Case, async: true

  alias OpakeAppview.SSE.TokenStore

  describe "create_token/1 and consume_token/1" do
    test "creates a token and consumes it" do
      token = TokenStore.create_token("did:plc:test")
      assert is_binary(token)
      assert byte_size(token) > 20

      assert {:ok, "did:plc:test"} = TokenStore.consume_token(token)
    end

    test "token is single-use" do
      token = TokenStore.create_token("did:plc:test")
      assert {:ok, _} = TokenStore.consume_token(token)
      assert :error = TokenStore.consume_token(token)
    end

    test "unknown token returns error" do
      assert :error = TokenStore.consume_token("nonexistent-token")
    end

    test "different DIDs get different tokens" do
      t1 = TokenStore.create_token("did:plc:alice")
      t2 = TokenStore.create_token("did:plc:bob")
      assert t1 != t2

      assert {:ok, "did:plc:alice"} = TokenStore.consume_token(t1)
      assert {:ok, "did:plc:bob"} = TokenStore.consume_token(t2)
    end
  end
end
