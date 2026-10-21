FROM rust:1.91-bookworm AS build
# Roha's .cargo/config.toml requires clang and lld on x86_64 Linux.
RUN apt-get update && apt-get install -y --no-install-recommends clang lld \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /source
COPY . .
RUN cargo build --locked --release -p iroh --example akula_chat_lab

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /source/target/release/examples/akula_chat_lab /app/lab
ENTRYPOINT ["/app/lab"]
CMD ["node"]
