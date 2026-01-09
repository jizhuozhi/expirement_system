.PHONY: build build-release run test clean proto

# Build targets
build:
	cargo build --features grpc

build-release:
	cargo build --release --features grpc

# Run server
run:
	RUST_LOG=info,experiment_data_plane=debug \
	GRPC_PORT=50051 \
	SERVER_PORT=8080 \
	cargo run --features grpc

# Test
test:
	cargo test --features grpc

# Clean
clean:
	cargo clean

# Regenerate proto (requires protoc)
proto:
	@echo "Protobuf will be regenerated on next build"
	rm -rf target/*/build/experiment-data-plane-*/out/

# Check gRPC health
health:
	@echo "Checking gRPC health..."
	@grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check || \
		echo "Server not running or grpcurl not installed (brew install grpcurl)"

# List gRPC services
services:
	@grpcurl -plaintext localhost:50051 list

# Example call
example:
	@echo "Calling GetExperiment..."
	@grpcurl -plaintext -d '{"services":["test"],"context":{"user_id":"123"}}' \
		localhost:50051 experiment.ExperimentService/GetExperiment

# Help
help:
	@echo "Available targets:"
	@echo "  build         - Build debug version"
	@echo "  build-release - Build release version"
	@echo "  run           - Run server locally"
	@echo "  test          - Run tests"
	@echo "  clean         - Clean build artifacts"
	@echo "  proto         - Force proto regeneration"
	@echo "  health        - Check gRPC health"
	@echo "  services      - List gRPC services"
	@echo "  example       - Example gRPC call"
