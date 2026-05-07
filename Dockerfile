FROM rust:1.85.0-bookworm AS builder

WORKDIR /build
COPY . .
RUN cargo build --release --bin cli --bin autominer_v3 --bin board_resetter_v3

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/cli /app/cli
COPY --from=builder /build/target/release/autominer_v3 /app/autominer_v3
COPY --from=builder /build/target/release/board_resetter_v3 /app/board_resetter_v3

ENTRYPOINT ["/app/cli"]
