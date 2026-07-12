# Self-contained build of the indigo relay (cmd/relay, Sync v1.1).
#
# WHY THIS EXISTS INSTEAD OF A GIT BUILD CONTEXT:
# indigo's own cmd/relay/Dockerfile runs `git describe --tags --long` to stamp
# the version. BuildKit's remote git build context checks the tree out WITHOUT
# a .git directory, so that command exits 128 and the build dies. We clone
# inside the build so .git (and tags) are present. Pin RELAY_REF to a commit or
# tag for reproducibility.
ARG RELAY_REF=main

# indigo main's go.mod requires go >= 1.26.
FROM golang:1.26-alpine AS build
ENV GOTOOLCHAIN=auto
RUN apk add --no-cache git build-base
ARG RELAY_REF
WORKDIR /src
RUN git clone https://github.com/bluesky-social/indigo.git . \
 && git fetch --tags --force \
 && git checkout "${RELAY_REF}"
# Cache mounts keep the module + build cache across rebuilds: a warm rebuild is
# seconds instead of a full recompile of indigo's dependency graph.
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    GIT_VERSION=$(git describe --tags --long --always) \
 && go build -tags timetzdata -ldflags "-X main.version=${GIT_VERSION}" -o /relay ./cmd/relay

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
COPY --from=build /relay /relay
ENTRYPOINT ["/relay"]
