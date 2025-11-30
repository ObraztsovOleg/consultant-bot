FROM rust:1.88.0 AS builder

WORKDIR /consultant-bot
COPY consultant-bot/ .
RUN cargo build --release

FROM ubuntu:22.04

RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /consultant-bot
COPY certs/russian_trusted_root_ca_pem.crt certs/russian_trusted_root_ca_gost_2025_pem.crt /usr/local/share/ca-certificates/

RUN update-ca-certificates -v
COPY --from=builder /consultant-bot/target/release/consultant-bot ./consultant-bot

ENTRYPOINT ["/consultant-bot/consultant-bot"]
