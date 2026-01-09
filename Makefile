.PHONY: help build test test-unit test-integration test-performance bench clean run dev fmt lint

# Test suite selection via TEST variable
# Usage:
#   make test                    - Run all tests (unit + integration)
#   make test-unit              - Run unit tests only
#   make test-integration       - Run integration tests
#   make test-performance       - Run performance tests
#   make bench                  - Run benchmarks (use SUITE= to select)
#   make bench SUITE=layer      - Layer management benchmarks
#   make bench SUITE=rule       - Rule evaluation benchmarks  
#   make bench SUITE=merge      - Param merge benchmarks

help:
	@echo "Available targets:"
	@echo ""
	@echo "🏗️  Build & Run:"
	@echo "  make build              - Build release binary"
	@echo "  make run                - Run data plane service"
	@echo "  make dev                - Run in development mode"
	@echo ""
	@echo "🧪 Testing:"
	@echo "  make test               - Run all tests (unit + integration)"
	@echo "  make test-unit          - Run unit tests only"
	@echo "  make test-integration   - Run integration tests"
	@echo "  make test-performance   - Run performance tests"
	@echo ""
	@echo "📊 Benchmarks:"
	@echo "  make bench              - Run all benchmarks"
	@echo "  make bench SUITE=layer  - Layer management (filtering, bucket calculation)"
	@echo "  make bench SUITE=rule   - Rule evaluation (depth, width, complexity)"
	@echo "  make bench SUITE=merge  - Param merge (layer count, depth, width, conflicts)"
	@echo ""
	@echo "🔧 Code Quality:"
	@echo "  make fmt                - Format code"
	@echo "  make lint               - Run clippy"
	@echo "  make clean              - Clean build artifacts"

build:
	@echo "🏗️  构建项目..."
	cd control_plane && make build
	cd data_plane && make build

test: test-unit test-integration
	@echo ""
	@echo "✅ 所有测试完成！"

test-unit:
	@echo "🧪 运行单元测试..."
	@chmod +x tests/unit.sh
	./tests/unit.sh

test-integration:
	@echo "🔗 运行集成测试..."
	@chmod +x tests/integration.sh
	./tests/integration.sh

test-performance:
	@echo "⚡ 运行性能测试..."
	@chmod +x tests/performance.sh
	./tests/performance.sh

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
	@echo "🎨 格式化代码..."
	cd control_plane && go fmt ./...
	cd data_plane && cargo fmt

lint:
	@echo "🔍 代码静态检查..."
	cd control_plane && go vet ./...
	cd data_plane && cargo clippy --features grpc -- -D warnings

clean:
	@echo "🧹 清理构建产物..."
	cd control_plane && make clean
	cd data_plane && make clean
	rm -rf data_plane/target/criterion
