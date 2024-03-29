FROM lukemathwalker/cargo-chef:latest-rust-1.63-bullseye AS chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN RUST_BACKTRACE=full cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

# We do not need the Rust toolchain to run the binary!
FROM ubuntu:22.10 AS runtime
WORKDIR /app

RUN mkdir /app/data
ENV COOKIES_PATH /app/data/cookies.json

RUN apt-get update && apt-get install -y chromium-browser

COPY --from=builder /app/target/release/decorp_bot /app/decorp_bot
ENTRYPOINT ["/app/decorp_bot"]