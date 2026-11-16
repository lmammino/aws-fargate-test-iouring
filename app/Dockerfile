# syntax=docker/dockerfile:1

# Build stage: official Rust toolchain on Debian 13 (glibc matches runtime).
FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
 && cp target/release/iouring-probe /iouring-probe

# Runtime stage: distroless glibc image, no shell, runs as uid 65532.
FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /iouring-probe /usr/local/bin/iouring-probe
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/iouring-probe"]
