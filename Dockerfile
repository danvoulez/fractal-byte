
FROM rust:1.75 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p ubl_gate

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/ubl_gate /usr/local/bin/ubl_gate
EXPOSE 3000
ENV REGISTRY_BASE_URL=http://localhost:3000
CMD ["ubl_gate"]
