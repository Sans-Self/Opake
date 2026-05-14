defmodule OpakeIndexer.Schemas.Grant do
  @moduledoc """
  An indexed `app.opake.grant` record. Grants are sharing permissions — the
  document's author authorizes a recipient to decrypt. The indexer indexes
  these from the Jetstream firehose so recipients can discover incoming
  grants via `/api/inbox`.

  `author_did` is the URI authority of the grant — by atproto rules, only
  the DID that hosts a document can write a grant for it, so author = the
  document's host.
  """

  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:uri, :string, autogenerate: false}
  schema "grants" do
    field :author_did, :string
    field :recipient_did, :string
    field :document_uri, :string
    field :created_at, :string
    field :indexed_at, :utc_datetime_usec
  end

  def changeset(grant, attrs) do
    grant
    |> cast(attrs, [:uri, :author_did, :recipient_did, :document_uri, :created_at, :indexed_at])
    |> validate_required([
      :uri,
      :author_did,
      :recipient_did,
      :document_uri,
      :created_at,
      :indexed_at
    ])
  end
end
