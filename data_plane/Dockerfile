# Build stage
FROM rust:1.75 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY benches ./benches
COPY tests ./tests

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/experiment-data-plane /usr/local/bin/

# Create layers directory
RUN mkdir -p /configs/layers

# Expose ports
EXPOSE 8080 9090

# Environment variables
ENV LAYERS_DIR=/configs/layers
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080
ENV METRICS_PORT=9090
ENV RUST_LOG=experiment_data_plane=info

# Run the binary
CMD ["experiment-data-plane"]
