# The shell, with a front end, in one image; and, from the same builder, one
# image per widget of the twenty-widget deployment (HLIN-I-0013).
#
# The release tarball is the binary alone, which starts, serves the API and has
# nothing to look at: building a front end needs the repository, `trunk`, and
# the wasm toolchain, which is not something a person deploying a service should
# have to acquire. This image has all of that at build time and none of it at
# run time.
#
# Stages, and the split is the point. The builder carries a Rust toolchain, a
# wasm target, `trunk`, wasm-bindgen and wasm-opt, and builds everything once
# (docker/build.sh); each runtime carries one binary and what it needs to run.
#
#   docker build .                                  the shell (the last stage)
#   docker build --build-arg TWENTY=1 \
#     --build-arg WIDGET=clock --target widget .    one widget
#
# deploy/twenty/compose.yml builds all twenty-one images that way, with the
# same TWENTY=1, so every one is the one builder's output, built once:
# BuildKit runs a stage once however many targets need it. The builder's
# cargo registry and target directory are cache mounts, so a rebuild after an
# edit recompiles what changed rather than everything.

# ---- Build ------------------------------------------------------------------

FROM rust:1.93-bookworm AS build

# `trunk` compiles the front end to WebAssembly, and needs a target rustup does
# not install by default.
RUN rustup target add wasm32-unknown-unknown

# From a release rather than `cargo install`: building trunk from source costs
# several minutes on every cache miss to produce a binary that is published
# ready-made.
#
# Chosen by the architecture actually being built for. This was hardcoded to
# x86_64 to begin with, which fails on an arm64 machine with exit 133 — an x86
# binary meeting an instruction it does not have — and would have worked on CI
# forever, because the runner is x86_64. A hardcoded arch here is a build that
# only ever works where somebody happened to test it.
ARG TRUNK_VERSION=0.21.7
ARG TARGETARCH
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) trunk_arch=x86_64 ;; \
      arm64) trunk_arch=aarch64 ;; \
      *) echo "no trunk release for ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-${trunk_arch}-unknown-linux-gnu.tar.gz" \
      | tar -xzf - -C /usr/local/bin trunk; \
    trunk --version

# The wasm-bindgen CLI, at the exact version the crate pins.
#
# Built here rather than left to trunk to fetch. Trunk downloads a release
# archive whose URL it constructs itself, and 0.21.7 constructs one that does
# not exist for aarch64 linux — a 404 in the middle of the build, for an asset
# the wasm-bindgen release does in fact publish. Compiling it costs a few
# minutes once, in a layer that caches, and depends on nothing but crates.io.
#
# The version must match `Trunk.toml`'s pin exactly: the CLI and the crate share
# a binary format that changes between versions.
ARG WASM_BINDGEN_VERSION=0.2.126
RUN cargo install wasm-bindgen-cli --version "${WASM_BINDGEN_VERSION}" --locked

# Binaryen, for `wasm-opt`, at the release every `Trunk.toml` pins. Installed
# here for the same reason: Trunk uses a matching one it finds on the PATH,
# and a download in the middle of forty parallel builds is a build that fails
# when GitHub is slow.
ARG BINARYEN_VERSION=version_123
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) binaryen_arch=x86_64 ;; \
      arm64) binaryen_arch=aarch64 ;; \
      *) echo "no binaryen release for ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/WebAssembly/binaryen/releases/download/${BINARYEN_VERSION}/binaryen-${BINARYEN_VERSION}-${binaryen_arch}-linux.tar.gz" \
      | tar -xzf - -C /tmp; \
    cp "/tmp/binaryen-${BINARYEN_VERSION}/bin/wasm-opt" /usr/local/bin/; \
    if [ -d "/tmp/binaryen-${BINARYEN_VERSION}/lib" ]; then \
      cp -R "/tmp/binaryen-${BINARYEN_VERSION}/lib/." /usr/local/lib/; \
    fi; \
    rm -rf "/tmp/binaryen-${BINARYEN_VERSION}"; \
    ldconfig; \
    wasm-opt --version

WORKDIR /src
COPY . .

# Whether to build the twenty widgets as well as the shell. Off for the
# shell's own image, which the release workflow builds and which should not
# wait on twenty servers it does not ship; on for deploy/twenty.
ARG TWENTY=0

# The registry and the target directory are cache mounts: kept between builds
# on this machine, so an edit recompiles the crates it touched. They are in no
# layer, which is why docker/build.sh copies what the runtimes need to /out.
RUN --mount=type=cache,id=hlin-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=hlin-target-${TARGETARCH},target=/src/target \
    TWENTY="${TWENTY}" sh docker/build.sh

# ---- Run --------------------------------------------------------------------

FROM debian:bookworm-slim AS base

# The shell reaches platforms over HTTPS and an identity provider over HTTPS,
# and a slim image has no roots to verify either against. `curl` because a
# widget fetches the shell's keys with it, and each image's healthcheck is
# one request.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Not root. The shell reads a config, writes one key file and listens; a
# widget only listens. Neither wants the privileges to change anything else.
RUN useradd --system --create-home --uid 10001 hlin

# ---- One widget ---------------------------------------------------------------

# One widget of the twenty: its server, with its own UI and its Hlin module
# compiled in. Built with TWENTY=1 and WIDGET=<name>.
FROM base AS widget

ARG WIDGET
COPY --from=build /out/bin/hlin-widget-${WIDGET} /usr/local/bin/widget

USER hlin
EXPOSE 8080

# Every widget takes the same flags (`hlin_widget_support::Common`), which
# the compose file gives.
ENTRYPOINT ["/usr/local/bin/widget"]

# ---- The shell ----------------------------------------------------------------

FROM base AS runtime

# The state directory has to exist in the image, owned by the user that will
# write to it. Docker creates a missing mount point as root, so declaring the
# volume without this gives a container that builds, starts, and dies on
# "key file: Permission denied" — which only running it finds.
RUN mkdir -p /home/hlin/state && chown hlin:hlin /home/hlin/state

USER hlin
WORKDIR /home/hlin

COPY --from=build /out/bin/hlin /usr/local/bin/hlin
COPY --from=build /out/frontend-gallery ./frontend
# The demo pack alone, which the twenty-widget surface is drawn with, as
# `demo up --with twenty` draws it. Empty unless built with TWENTY=1, so the
# released image carries only the gallery.
COPY --from=build /out/frontend-demo ./frontend-demo
COPY docker/hlin.toml /etc/hlin/hlin.toml

# Where the signing key lands, so a restart keeps its `kid` and the tokens it
# minted stay verifiable. Mount a volume here or every restart is a new issuer.
VOLUME ["/home/hlin/state"]

EXPOSE 8080

# Overridable: a deployment with its own platforms, its own identity provider
# and its own design pack replaces this file and changes nothing else.
ENV HLIN_CONFIG=/etc/hlin/hlin.toml

ENTRYPOINT ["/bin/sh", "-c", "exec hlin serve --config \"$HLIN_CONFIG\""]
