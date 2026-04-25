defmodule OpakeIndexer.Repo.Migrations.CreateCursor do
  use Ecto.Migration

  def change do
    create table(:cursor, primary_key: false) do
      add :id, :integer, primary_key: true
      add :time_us, :bigint, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    execute "ALTER TABLE cursor ADD CONSTRAINT cursor_singleton CHECK (id = 1)", ""
  end
end
