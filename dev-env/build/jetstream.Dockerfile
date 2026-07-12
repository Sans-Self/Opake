# jetstream built from source (bluesky-social/jetstream main). We build from
# source rather than the published sha images because those are the frozen
# v0.1.0 with a hardcoded 15s idle self-kill; main is the maintained rewrite
# without it. Pin JS_REF to a commit for reproducibility.
ARG JS_REF=c1d576e987eb6a9de4da948a03ad5aaeb165a7b0

FROM golang:1.26.4-alpine AS build
RUN apk add --no-cache git
ARG JS_REF
WORKDIR /src
RUN git clone https://github.com/bluesky-social/jetstream.git . \
 && git checkout "${JS_REF}"
ENV CGO_ENABLED=0
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    go build -trimpath -buildvcs=false -o /jetstream ./cmd/jetstream

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
COPY --from=build /jetstream /jetstream
ENTRYPOINT ["/jetstream"]
