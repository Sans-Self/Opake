defmodule OpakeAppviewWeb.ErrorJSON do
  def render("400.json", _assigns), do: %{error: "bad request"}
  def render("401.json", _assigns), do: %{error: "unauthorized"}
  def render("404.json", _assigns), do: %{error: "not found"}
  def render("429.json", _assigns), do: %{error: "rate limit exceeded"}
  def render("500.json", _assigns), do: %{error: "internal server error"}

  def render(template, _assigns) do
    %{error: Phoenix.Controller.status_message_from_template(template)}
  end
end
