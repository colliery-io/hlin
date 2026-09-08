# The shell, with a front end, in one image.
#
# The release tarball is the binary alone, which starts, serves the API and has
# nothing to look at: building a front end needs the repository, `trunk`, and
# the wasm toolchain, which is not something a person deploying a service should
# have to acquire. This image has all of that at build time and none of it at
# run time.
#
# Two stages, and the split is the point. The builder carries a Rust toolchain,
# a wasm target and `trunk`; the runtime carries a binary, a directory of static
# files and the certificates needed to reach a platform over TLS.

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

WORKDIR /src
COPY . .

# The gallery front end, because it is built from more than one design pack and
# `?pack=` chooses between them — an image somebody is evaluating should be able
# to show that the shell owns no design system, which is the whole claim.
RUN cd examples/frontend-gallery && trunk build --release

RUN cargo build --release -p hlin

# ---- Run --------------------------------------------------------------------

FROM debian:bookworm-slim AS runtime

# The shell reaches platforms over HTTPS and an identity provider over HTTPS,
# and a slim image has no roots to verify either against.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Not root. The shell reads a config, writes one key file and listens; none of
# that wants the privileges to change anything else in the container.
RUN useradd --system --create-home --uid 10001 hlin

# The state directory has to exist in the image, owned by the user that will
# write to it. Docker creates a missing mount point as root, so declaring the
# volume without this gives a container that builds, starts, and dies on
# "key file: Permission denied" — which only running it finds.
RUN mkdir -p /home/hlin/state && chown hlin:hlin /home/hlin/state

USER hlin
WORKDIR /home/hlin

COPY --from=build /src/target/release/hlin /usr/local/bin/hlin
COPY --from=build /src/examples/frontend-gallery/dist ./frontend
COPY docker/hlin.toml /etc/hlin/hlin.toml

# Where the signing key lands, so a restart keeps its `kid` and the tokens it
# minted stay verifiable. Mount a volume here or every restart is a new issuer.
VOLUME ["/home/hlin/state"]

EXPOSE 8080

# Overridable: a deployment with its own platforms, its own identity provider
# and its own design pack replaces this file and changes nothing else.
ENV HLIN_CONFIG=/etc/hlin/hlin.toml

ENTRYPOINT ["/bin/sh", "-c", "exec hlin serve --config \"$HLIN_CONFIG\""]
