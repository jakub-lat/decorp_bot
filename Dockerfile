FROM rustlang/rust:slim AS planner
WORKDIR /app

RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM rustlang/rust:slim AS cacher
WORKDIR /app

RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rustlang/rust:slim AS builder
WORKDIR /app

COPY . .
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --release --bin bot

FROM debian:buster-slim

WORKDIR /home/site/wwwroot
# RUN apt-get update && apt-get install -y ca-certificates openssh-server sudo && update-ca-certificates

COPY --from=builder /app/target/release/bot /usr/local/bin

CMD bot
