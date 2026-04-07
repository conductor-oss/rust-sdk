# --- Stage 1: build the SDK library and cache dependencies -----------------
#
# This stage compiles all dependencies + the SDK and caches the result as a
# Docker layer.  The harness .rs source is NOT copied here — only its
# Cargo.toml (with a throwaway main.rs stub) so Cargo sees a valid workspace.
#
# The payoff: when only harness source files change, this entire layer is
# cached and stage 2 recompiles just the harness binary.  Without the split,
# every harness edit would recompile the SDK and all ~100 transitive deps.
#
# The alternative tool for this pattern is cargo-chef, but it adds a build
# dependency that has to be installed inside the image and kept up to date.
# The stub approach accomplishes the same thing with zero extra tooling.
#
FROM rust:1.86-bookworm AS build
RUN mkdir -p /package/harness/src
COPY /src /package/src
COPY /conductor-macros /package/conductor-macros
COPY /harness/Cargo.toml /package/harness/Cargo.toml
COPY /Cargo.toml /Cargo.lock /package/
RUN echo 'fn main() {}' > /package/harness/src/main.rs
WORKDIR /package
RUN cargo build --release

# --- Stage 2: build the harness binary ------------------------------------
#
# Copies the real harness source, overwriting the stub.  Only the harness
# crate recompiles; the SDK and all dependencies are cached from stage 1.
#
FROM build AS harness-build
COPY /harness /package/harness
WORKDIR /package
RUN cargo build --release -p harness

# --- Stage 3: minimal runtime image ---------------------------------------
FROM debian:bookworm-slim AS harness
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN adduser --disabled-password --uid 65532 --gecos "" nonroot
USER nonroot
COPY --from=harness-build /package/target/release/harness /app/harness
WORKDIR /app
ENTRYPOINT ["/app/harness"]
