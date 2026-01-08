.PHONY: help build test bench clean run dev fmt lint

# Benchmark suite selection via SUITE variable
# Usage:
#   make bench              - Run all benchmarks
#   make bench SUITE=layer  - Layer management (filtering, bucket calculation)
#   make bench SUITE=rule   - Rule evaluation (depth, width, complexity)
#   make bench SUITE=merge  - Param merge (layer count, depth, width, conflicts)

help:
	@echo "Available targets:"
	@echo "  make build       - Build release binary"
	@echo "  make test        - Run all tests"
	@echo "  make bench       - Run benchmarks (use SUITE= to select)"
	@echo "                     SUITE=layer|rule|merge (default: all)"
	@echo "  make run         - Run data plane service"
	@echo "  make dev         - Run in development mode"
	@echo "  make fmt         - Format code"
	@echo "  make lint        - Run clippy"
	@echo "  make clean       - Clean build artifacts"

build:
	cd data_plane && cargo build --release

test:
	cd data_plane && cargo test

bench:
ifeq ($(SUITE),layer)
	@echo "📊 Running layer management benchmarks..."
	@echo "   - Layer filtering by service"
	@echo "   - Bucket calculation"
	@echo "   - Layer priority sorting"
	@echo ""
	cd data_plane && cargo bench --bench layer_management_bench
else ifeq ($(SUITE),rule)
	@echo "🧠 Running rule evaluation benchmarks..."
	@echo "   - Simple rules (eq, in, gte)"
	@echo "   - Rule depth (2-20 levels)"
	@echo "   - Rule width (5-100 conditions)"
	@echo "   - Complex patterns"
	@echo "   - Batch evaluation (10-5k rules)"
	@echo ""
	cd data_plane && cargo bench --bench rule_evaluation_bench
else ifeq ($(SUITE),merge)
	@echo "💥 Running param merge benchmarks..."
	@echo "   - Layer count (10-10k layers)"
	@echo "   - Param depth (1-15 levels)"
	@echo "   - Param width (5-100 fields)"
	@echo "   - Extreme merge (up to 5k layers, 5 levels deep, 25 fields)"
	@echo "   - Conflict resolution"
	@echo ""
	cd data_plane && cargo bench --bench param_merge_bench
else
	@echo "🚀 Running all benchmarks..."
	@echo ""
	cd data_plane && cargo bench
endif
	@echo ""
	@echo "✅ Benchmarks completed!"
	@echo "📈 View results: open data_plane/target/criterion/report/index.html"

run:
	cd data_plane && LAYERS_DIR=../configs/layers cargo run --release

dev:
	cd data_plane && RUST_LOG=debug LAYERS_DIR=../configs/layers cargo run

fmt:
	cd data_plane && cargo fmt

lint:
	cd data_plane && cargo clippy -- -D warnings

clean:
	cd data_plane && cargo clean
	rm -rf data_plane/target/criterion
