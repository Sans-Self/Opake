# Opake indexer — hermetic dev-env image.
#
# Build context is apps/indexer (this Dockerfile lives in dev-env/build/):
#   docker build -f dev-env/build/indexer.Dockerfile \
#     -t opake-devenv-indexer:pinned apps/indexer
#
# Produces a prod `mix release` (release name: opake_indexer). Runtime config
# is read at RUN time from the environment (see config/runtime.exs): the build
# does NOT require DATABASE_URL / SECRET_KEY_BASE.
#
# The app serves on $PORT (default 6100) and exposes a public health route at
# GET /api/health (see lib/opake_indexer_web/router.ex). curl is installed in
# the runtime stage so the HEALTHCHECK below can probe it. If PORT is remapped
# away from 6100 at run time, the HEALTHCHECK URL (hardcoded to 6100) must be
# overridden in compose.

# ---- Build stage -----------------------------------------------------------
# Pinned: elixir 1.15.8 + erlang 26.2.5.2 + debian bookworm (20240701).
FROM hexpm/elixir:1.15.8-erlang-26.2.5.2-debian-bookworm-20240701-slim AS build

RUN apt-get update -y \
  && apt-get install -y --no-install-recommends build-essential git \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Prod build. runtime.exs only raises on missing DATABASE_URL/SECRET_KEY_BASE,
# which is evaluated at boot, not at compile time.
ENV MIX_ENV=prod

RUN mix local.hex --force && mix local.rebar --force

# Dependency layer — cached until mix.exs / mix.lock change.
COPY mix.exs mix.lock ./
RUN mix deps.get --only prod
RUN mix deps.compile

# Compile-time config (config.exs, prod.exs) feeds the release; copy before
# compiling the app so config changes invalidate at the right layer.
COPY config config
COPY priv priv
COPY lib lib

RUN mix compile

# Runtime config + release entrypoint.
COPY rel rel

RUN mix release

# ---- Runtime stage ---------------------------------------------------------
# Debian date matches the build image's bookworm snapshot (20240701).
FROM debian:bookworm-20240701-slim AS runtime

RUN apt-get update -y \
  && apt-get install -y --no-install-recommends \
       libstdc++6 openssl libncurses6 locales ca-certificates curl \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/* \
  && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen \
  && locale-gen

# Elixir requires a UTF-8 locale.
ENV LANG=en_US.UTF-8 \
    LANGUAGE=en_US:en \
    LC_ALL=en_US.UTF-8

WORKDIR /app

# The release ships its own ERTS; no Erlang/Elixir install needed at runtime.
COPY --from=build /app/_build/prod/rel/opake_indexer ./
COPY --from=build /app/rel/entrypoint.sh ./entrypoint.sh
RUN chmod +x ./entrypoint.sh

EXPOSE 6100

# Health-gate: probe the public health route. start-period covers create_db +
# migrate + boot before failures count against retries.
HEALTHCHECK --interval=5s --timeout=3s --start-period=40s --retries=30 \
  CMD curl -fsS http://localhost:6100/api/health || exit 1

# entrypoint.sh runs create_db + migrate (needs a reachable DB) then
# `bin/opake_indexer start`. WORKDIR is the release root so `bin/opake_indexer`
# resolves.
ENTRYPOINT ["./entrypoint.sh"]
