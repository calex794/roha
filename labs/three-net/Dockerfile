FROM rust:1.91-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang lld && rm -rf /var/lib/apt/lists/*
WORKDIR /source
COPY . .
RUN cargo build --locked -p iroh --example akula_three_net_lab
FROM debian:bookworm-slim
COPY --from=build /source/target/debug/examples/akula_three_net_lab /app/lab
ENTRYPOINT ["/app/lab"]
CMD ["node"]
