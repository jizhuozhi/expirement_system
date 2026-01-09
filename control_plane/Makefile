.PHONY: help build run dev proto migrate test clean

help:
	@echo "Available commands:"
	@echo "  make build      - Build the control plane"
	@echo "  make run        - Run the control plane"
	@echo "  make dev        - Run in development mode"
	@echo "  make proto      - Generate protobuf code"
	@echo "  make migrate    - Run database migrations"
	@echo "  make test       - Run tests"
	@echo "  make clean      - Clean build artifacts"

build: proto
	@echo "Building control plane..."
	cd cmd/server && go build -o ../../bin/control-plane

run: build
	@echo "Starting control plane..."
	./bin/control-plane -config config.yaml

dev: proto
	@echo "Running in development mode..."
	go run cmd/server/main.go -config config.yaml

proto:
	@echo "Generating protobuf code..."
	@mkdir -p proto/config/v1
	protoc --go_out=. --go_opt=paths=source_relative \
		--go-grpc_out=. --go-grpc_opt=paths=source_relative \
		-I=../proto \
		../proto/config_push.proto
	@echo "Generated xDS proto files:"
	@echo "  configv1 \"github.com/georgeji/experiment-system/proto/config/v1\""

migrate:
	@echo "Running migrations..."
	psql -h localhost -U postgres -d experiment_control -f migrations/001_init.sql

test: proto
	@echo "Running tests..."
	go test -v ./...

clean:
	@echo "Cleaning..."
	rm -rf bin/
	rm -rf proto/
	go clean
