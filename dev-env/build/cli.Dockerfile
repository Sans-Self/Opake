# Opake CLI built into a container so bootstrap can run INSIDE the internal
# (egress-blocked) network and reach the PDSes over plain http — keeping the
# PDSes strictly internal-only (no host ports, no egress). Also carries curl +
# jq + bash so the bootstrap script (mounted at run time) can mint invites and
# create accounts against the PDS admin/XRPC API.
# Recent toolchain required: some deps (e.g. base64ct 1.8.x) use edition 2024.
FROM rust:1.88-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config git && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY apps/cli apps/cli
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p opake-cli && cp target/release/opake /opake

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      curl jq bash ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /opake /usr/local/bin/opake
ENTRYPOINT ["/bin/bash"]
