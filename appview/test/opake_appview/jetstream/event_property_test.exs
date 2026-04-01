defmodule OpakeAppview.Jetstream.EventPropertyTest do
  @moduledoc """
  Property-based tests for the Jetstream event parser.
  Ensures parse/1 never crashes regardless of input shape.
  """

  use ExUnit.Case, async: true
  use ExUnitProperties

  alias OpakeAppview.Jetstream.Event

  @valid_event_tags [
    :upsert_grant,
    :delete_grant,
    :upsert_keyring,
    :delete_keyring,
    :upsert_workspace_document,
    :delete_workspace_document,
    :upsert_document_update,
    :delete_document_update,
    :keyring_leave
  ]

  property "parse/1 never crashes on arbitrary binaries" do
    check all(input <- binary()) do
      result = Event.parse(input)
      assert result == :ignore or match?({tag, _} when tag in @valid_event_tags, result)
    end
  end

  property "parse/1 never crashes on random JSON maps" do
    check all(
            map <-
              map_of(
                string(:alphanumeric, min_length: 1),
                one_of([
                  integer(),
                  float(),
                  string(:alphanumeric),
                  boolean(),
                  constant(nil)
                ]),
                min_length: 0,
                max_length: 10
              )
          ) do
      json = Jason.encode!(map)
      result = Event.parse(json)
      assert result == :ignore or match?({tag, _} when tag in @valid_event_tags, result)
    end
  end

  property "parse/1 on valid grant JSON returns :upsert_grant or :ignore" do
    check all(
            did <- did_generator(),
            rkey <- rkey_generator(),
            recipient <- did_generator(),
            doc_rkey <- rkey_generator(),
            time_str <- time_string_generator()
          ) do
      json =
        Jason.encode!(%{
          "did" => did,
          "time_us" => 1_709_330_400_000_000,
          "kind" => "commit",
          "commit" => %{
            "rev" => "abc",
            "operation" => "create",
            "collection" => "app.opake.grant",
            "rkey" => rkey,
            "record" => %{
              "recipient" => recipient,
              "document" => "at://#{did}/app.opake.document/#{doc_rkey}",
              "createdAt" => time_str
            }
          }
        })

      result = Event.parse(json)
      assert result == :ignore or match?({:upsert_grant, _}, result)
    end
  end

  # -- Generators --

  defp did_generator do
    map(string(:alphanumeric, min_length: 3, max_length: 20), &"did:plc:#{&1}")
  end

  defp rkey_generator do
    string(:alphanumeric, min_length: 3, max_length: 13)
  end

  defp time_string_generator do
    constant("2026-03-01T12:00:00Z")
  end
end
