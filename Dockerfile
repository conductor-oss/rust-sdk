FROM rust:1.85-bookworm AS build
RUN mkdir /package
COPY /src /package/src
COPY /conductor-macros /package/conductor-macros
COPY /Cargo.toml /package/Cargo.toml
WORKDIR /package
# Pre-build the SDK library; the harness workspace member isn't present yet,
# so temporarily remove it from the workspace members list.
RUN sed -i 's/, "harness"//' Cargo.toml && cargo build --release

FROM build AS harness-build
COPY /harness /package/harness
# Restore the workspace member now that the directory exists
RUN sed -i '/members/s/"conductor-macros"/"conductor-macros", "harness"/' Cargo.toml
WORKDIR /package
RUN cargo build --release -p harness

FROM debian:bookworm-slim AS harness
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN adduser --disabled-password --uid 65532 --gecos "" nonroot
USER nonroot
COPY --from=harness-build /package/target/release/harness /app/harness
WORKDIR /app
ENTRYPOINT ["/app/harness"]
