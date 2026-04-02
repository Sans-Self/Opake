defmodule OpakeAppview.Repo do
  use Ecto.Repo,
    otp_app: :opake_appview,
    adapter: Ecto.Adapters.Postgres
end
