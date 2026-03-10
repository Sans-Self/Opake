defmodule OpakeAppview.Schemas.Grant do
  @moduledoc """
  An indexed `app.opake.grant` record. Grants are sharing permissions — the
  owner authorizes a recipient to decrypt a specific document. The appview
  indexes these from the Jetstream firehose so recipients can discover incoming
  grants via the `/api/inbox` endpoint.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:uri, :string, autogenerate: false}
  schema "grants" do
    field :owner_did, :string
    field :recipient_did, :string
    field :document_uri, :string
    field :created_at, :string
    field :indexed_at, :utc_datetime_usec
  end

  def changeset(grant, attrs) do
    grant
    |> cast(attrs, [:uri, :owner_did, :recipient_did, :document_uri, :created_at, :indexed_at])
    |> validate_required([:uri, :owner_did, :recipient_did, :document_uri, :created_at, :indexed_at])
  end
end
