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
    :upsert_directory,
    :delete_directory,
    :upsert_document,
    :delete_document,
    :upsert_document_update,
    :delete_document_update,
    :upsert_directory_update,
    :delete_directory_update,
    :upsert_keyring_update,
    :delete_keyring_update
  ]

  defp valid_result?(result) do
    case result do
      {time_us, collection, :ignore}
      when (is_integer(time_us) or is_nil(time_us)) and
             (is_binary(collection) or is_nil(collection)) ->
        true

      {time_us, collection, {tag, attrs}}
      when (is_integer(time_us) or is_nil(time_us)) and
             (is_binary(collection) or is_nil(collection)) and
             is_map(attrs) ->
        tag in @valid_event_tags

      _ ->
        false
    end
  end

  property "parse/1 never crashes on arbitrary binaries" do
    check all(input <- binary()) do
      assert valid_result?(Event.parse(input))
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
      assert valid_result?(Event.parse(json))
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
      assert valid_result?(result)

      assert match?({1_709_330_400_000_000, "app.opake.grant", :ignore}, result) or
               match?({1_709_330_400_000_000, "app.opake.grant", {:upsert_grant, _}}, result)
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
