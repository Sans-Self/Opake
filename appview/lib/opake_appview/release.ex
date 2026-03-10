defmodule OpakeAppview.Release do
  @moduledoc """
  Release tasks for running outside of Mix (e.g. in a Docker container).

  - `create_db/0` — creates the database if it doesn't exist
  - `migrate/0` — runs pending Ecto migrations
  - `rollback/2` — rolls back to a specific migration version
  - `status/0` — prints cursor position, lag, and indexed record counts
  """

  require Ecto.Query

  @app :opake_appview

  def create_db do
    load_app()

    for repo <- repos() do
      case repo.__adapter__().storage_up(repo.config()) do
        :ok -> IO.puts("Database created")
        {:error, :already_up} -> IO.puts("Database already exists")
        {:error, reason} -> raise "Could not create database: #{inspect(reason)}"
      end
    end
  end

  def migrate do
    load_app()

    for repo <- repos() do
      {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :up, all: true))
    end
  end

  def rollback(repo, version) do
    load_app()
    {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :down, to: version))
  end

  def status do
    load_app()

    {:ok, _, _} =
      Ecto.Migrator.with_repo(OpakeAppview.Repo, fn repo ->
        cursor = repo.get(OpakeAppview.Schemas.Cursor, 1)

        grant_count =
          repo.aggregate(OpakeAppview.Schemas.Grant, :count)

        keyring_count =
          repo.one(
            Ecto.Query.from(km in OpakeAppview.Schemas.KeyringMember,
              select: count(km.keyring_uri, :distinct)
            )
          )

        IO.puts("Cursor:")

        case cursor do
          nil ->
            IO.puts("  (none)")

          %{time_us: time_us, updated_at: updated_at} ->
            cursor_time = DateTime.from_unix!(time_us, :microsecond)
            now = DateTime.utc_now()
            lag_secs = DateTime.diff(now, cursor_time)
            IO.puts("  Position: #{time_us}")
            IO.puts("  Time: #{DateTime.to_iso8601(cursor_time)}")
            IO.puts("  Lag: #{lag_secs}s")
            IO.puts("  Updated: #{DateTime.to_iso8601(updated_at)}")
        end

        IO.puts("\nCounts:")
        IO.puts("  Grants: #{grant_count}")
        IO.puts("  Keyrings: #{keyring_count}")
      end)
  end

  defp repos do
    Application.fetch_env!(@app, :ecto_repos)
  end

  defp load_app do
    Application.ensure_all_started(@app)
  end
end
