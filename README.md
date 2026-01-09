# High-Performance Distributed Experiment System

A high-performance, low-latency A/B testing and experiment management system implemented in Rust, supporting layered experiments, complex rule engines, hot updates, and fine-grained traffic control.

## Core Features

- ✅ **10,000 Hash Slots**: 0.01% traffic granularity with XXHash algorithm
- ✅ **Multi-layer Parameter Merge**: Priority-based control with recursive deep merge
- ✅ **Deterministic Bucketing**: Sticky bucketing + salt mechanism ensures experiment independence
- ✅ **Traffic Splitting**: Range mechanism supports namespace mutual exclusion experiments
- ✅ **Rule Engine**: 13 operators with arbitrary depth AND/OR nesting
- ✅ **Hot Updates**: < 100ms with Arc + RwLock atomic replacement
- ✅ **High Performance**: Single core > 100K QPS, P50 < 1ms, P99 < 5ms
- ✅ **Zero Copy**: Arc shared data structures, no GC pauses
- ✅ **Observability**: Prometheus metrics + detailed logging

## Architecture

```
┌──────────────────────────────────────┐
│         UI Service (Port 8081)       │
│  - Next.js / React                   │
│  - OIDC Provider (User Auth)         │
│  - RBAC (admin/developer/viewer)     │
│  - Audit Logs                        │
│  - API Gateway                       │
└──────────────┬───────────────────────┘
               │ Internal gRPC/HTTP
               │ (Service Account Token)
               ↓
┌──────────────────────────────────────┐
│    Control Plane (Port 8082)         │
│  - Layer/Experiment CRUD             │
│  - Configuration Version Management  │
│  - AK/SK Authentication (Service)    │
│  - gRPC Config Push Service          │
└──────────────┬───────────────────────┘
               │
               ├─→ PostgreSQL (Config Storage)
               │
               └─→ gRPC Stream (Port 9091)
                   (AK/SK Auth)
                   ↓
            ┌──────────────────┐
            │   Data Plane     │
            │  - Rust Service  │
            │  - Experiment    │
            │  - Hot Updates   │
            └──────────────────┘
```

## Quick Start

### Build and Run

```bash
# Build release version
make build

# Run data plane service
make run

# Development mode (with logs)
make dev
```

Services listen on:
- HTTP API: `http://localhost:8080`
- Metrics: `http://localhost:9090/metrics`

### Test API

```bash
curl -X POST http://localhost:8080/experiment \
  -H "Content-Type: application/json" \
  -d '{
    "services": ["ranker_svc"],
    "context": {
      "user_id": "user_12345",
      "country": "US",
      "age": 25
    }
  }'
```

Response example:

```json
{
  "results": {
    "ranker_svc": {
      "parameters": {
        "algorithm": "gbdt",
        "timeout_ms": 150,
        "model_version": "v2.1"
      },
      "vids": [1001, 1002],
      "matched_layers": ["layer_1", "layer_2"]
    }
  }
}
```

## Core Modules

### Data Plane (Rust)

High-performance experiment evaluation service:

```
src/
├── config/          # Configuration management
├── engine/          # Core engine (catalog, layer, rule, merge, hash)
├── server/          # Service layer (HTTP + gRPC)
├── utils/           # Utilities (error, metrics)
└── xds_client.rs    # xDS client
```

**Key Features:**
- Zero-copy data sharing with Arc + RwLock
- XXHash for consistent bucketing
- Recursive parameter merging
- Real-time configuration updates

### Control Plane (Go)

Configuration management and push service:

```
control_plane/
├── cmd/server/          # Main program
├── internal/
│   ├── config/          # Unified config manager
│   ├── grpc_server/     # gRPC push service
│   ├── repository/      # Data access layer
│   └── models/          # Data models
├── pkg/auth/            # AK/SK authentication
└── migrations/          # Database migrations
```

**Key Features:**
- PostgreSQL with JSONB for complex configurations
- AK/SK authentication for service-level access
- Real-time configuration push via gRPC streams
- Transaction-based changelog for consistency

## Performance Benchmarks

### Test Environment
- **CPU**: Apple M1/M2 or Intel Xeon
- **Memory**: 16GB+
- **Compilation**: Rust 1.70+ Release mode with `-O3` + LTO

### Benchmark Results

#### Layer Management
| Test | Scale | Performance | Description |
|------|-------|-------------|-------------|
| Layer filtering | 1K layers | ~3µs | Filter by service |
| Layer filtering | 10K layers | ~35µs | Filter by service |
| Bucket calculation | Single | ~98ns | XXHash calculation |
| Layer sorting | 1K layers | ~55µs | Priority-based access |

#### Parameter Merge
| Test | Scale | Performance | Description |
|------|-------|-------------|-------------|
| Layer count | 10 layers | ~820µs | Basic merge |
| Layer count | 100 layers | ~6.8ms | Medium scale |
| Layer count | 1000 layers | ~68ms | Large scale |
| Param depth | 1 level | ~42µs | Shallow nesting |
| Param depth | 3 levels | ~6.6ms | Nested objects |

#### End-to-End Performance
| Scenario | P50 | P99 | Description |
|----------|-----|-----|-------------|
| Simple (10 layers) | < 1ms | < 3ms | Basic experiments |
| Medium (100 layers) | < 10ms | < 20ms | Complex experiments |
| Single core QPS | > 100K | - | Lightweight requests |

## Configuration Examples

### Layer Configuration

```json
{
  "layer_id": "click_experiment",
  "version": "v1",
  "priority": 200,
  "hash_key": "user_id",
  "salt": "click_exp_2024",
  "enabled": true,
  "ranges": [
    {"start": 0, "end": 5000, "vid": 1001},
    {"start": 5000, "end": 10000, "vid": 1002}
  ]
}
```

### Experiment Configuration

```json
{
  "eid": 100,
  "service": "ranker_svc",
  "rule": {
    "type": "and",
    "children": [
      {"type": "field", "field": "country", "op": "eq", "values": ["US"]},
      {"type": "field", "field": "age", "op": "gte", "values": [18]}
    ]
  },
  "variants": [
    {
      "vid": 1001,
      "params": {"algorithm": "baseline", "timeout": 100}
    },
    {
      "vid": 1002,
      "params": {"algorithm": "new_model", "timeout": 200}
    }
  ]
}
```

## Rule Engine

### Supported Operators
- **Comparison**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`
- **Set**: `in`, `not_in`
- **String**: `like`, `not_like` (supports `*` wildcards)
- **Logic**: `and`, `or`, `not`

### Field Types
- `string` - String values
- `int` - Integer values
- `float` - Float values
- `bool` - Boolean values
- `semver` - Semantic versions

## Testing

### Run Tests

```bash
# Run all tests
make test

# Run specific test suites
make test-unit          # Unit tests
make test-integration   # Integration tests
make test-performance   # Performance tests

# Run benchmarks
make bench              # All benchmarks
make bench SUITE=layer  # Layer management benchmarks
make bench SUITE=rule   # Rule evaluation benchmarks
make bench SUITE=merge  # Parameter merge benchmarks
```

### Test Coverage

**Functional Coverage:**
- ✅ Control plane Go code tests
- ✅ Data plane Rust code tests
- ✅ xDS protocol integration tests
- ✅ HTTP/gRPC API tests
- ✅ Performance and load tests

**Quality Assurance:**
- ✅ Code formatting checks
- ✅ Static analysis (go vet, clippy)
- ✅ Documentation tests
- ✅ Proto syntax validation

## Deployment

### Sidecar Mode (Recommended)

Deploy with business services in the same Pod for lowest latency:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: my-app
spec:
  containers:
  - name: app
    image: my-app:latest
    env:
    - name: EXPERIMENT_SERVICE_URL
      value: "http://localhost:8080"
  
  - name: experiment-sidecar
    image: experiment-data-plane:latest
    ports:
    - containerPort: 8080
    - containerPort: 9090
    env:
    - name: LAYERS_DIR
      value: "/configs/layers"
    - name: EXPERIMENTS_DIR
      value: "/configs/experiments"
    volumeMounts:
    - name: config
      mountPath: /configs
  
  volumes:
  - name: config
    configMap:
      name: experiment-config
```

### Standalone Deployment

Deploy as independent service for multiple business services:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: experiment-data-plane
spec:
  replicas: 3
  selector:
    matchLabels:
      app: experiment-data-plane
  template:
    metadata:
      labels:
        app: experiment-data-plane
    spec:
      containers:
      - name: data-plane
        image: experiment-data-plane:latest
        ports:
        - containerPort: 8080
          name: http
        - containerPort: 9090
          name: metrics
        resources:
          requests:
            cpu: "500m"
            memory: "128Mi"
          limits:
            cpu: "2000m"
            memory: "512Mi"
```

## Monitoring

### Prometheus Metrics

System exposes the following metrics:

```promql
# QPS
rate(experiment_requests_total[1m])

# Error rate
rate(experiment_request_errors_total[1m]) / rate(experiment_requests_total[1m])

# P50/P99 latency
histogram_quantile(0.50, rate(experiment_request_duration_seconds_bucket[1m]))
histogram_quantile(0.99, rate(experiment_request_duration_seconds_bucket[1m]))

# Layer reload count
rate(experiment_layer_reload_total[1m])

# Rule evaluation count
rate(experiment_rule_evaluations_total[1m])
```

### Logging

Use `RUST_LOG` environment variable to control log levels:

```bash
# Development mode with detailed logs
RUST_LOG=debug cargo run

# Production mode with error logs only
RUST_LOG=error cargo run

# Show specific module logs
RUST_LOG=experiment_data_plane::merge=debug cargo run
```

## Best Practices

### Layer Design
1. **Priority Intervals**: Use 100, 200, 300 for easy insertion
2. **Explicit Service**: Specify `service` field to limit scope
3. **Independent Salt**: Use different salt for each layer
4. **Clear Naming**: Use descriptive `layer_id`

### Traffic Allocation
1. **Choose hash_key**: Use evenly distributed fields (like user_id)
2. **Avoid Sequential Fields**: Don't use timestamps or sequential values
3. **Maintain Salt on Scale**: Only modify ranges when scaling, keep salt unchanged
4. **Gradual Rollout**: Scale 1% → 5% → 10% → 50% → 100%

### Parameter Design
1. **Control Size**: Single layer parameters < 1KB
2. **Structured Organization**: Use nested objects for related parameters
3. **Avoid Large Data**: Don't include large data or lists in parameters
4. **Type Consistency**: Keep same parameter types across layers

## Development Commands

```bash
# View all commands
make help

# Build
make build          # Release build
make test           # Run tests

# Run services
make run            # Production mode
make dev            # Development mode (with logs)

# Benchmarks
make bench          # All tests
make bench SUITE=layer   # Layer management tests
make bench SUITE=rule    # Rule evaluation tests
make bench SUITE=merge   # Parameter merge tests

# Code quality
make fmt            # Format code
make lint           # Clippy checks
make clean          # Clean build artifacts
```

## Technology Stack

- **Web Framework**: Axum + Tower (high-performance async HTTP)
- **Concurrency**: Tokio (async runtime)
- **Serialization**: Serde (JSON/YAML)
- **Hashing**: XXHash (high-performance hash algorithm)
- **Monitoring**: Prometheus (metrics)
- **Logging**: Tracing (structured logging)
- **Concurrency Primitives**: Arc + RwLock (zero-copy sharing)
- **File Watching**: Notify (hot updates)

## Roadmap

### Short Term (1-2 months)
- [ ] gRPC protocol support
- [ ] Configuration validation and dry-run mode
- [ ] Richer metrics (rule evaluation time, parameter merge time)
- [ ] Remote configuration center support (etcd/Consul)

### Medium Term (3-6 months)
- [ ] Control plane Web UI
  - Layer configuration generation and validation
  - Visual traffic allocation
  - Experiment state management
- [ ] Configuration version management and rollback
- [ ] A/B testing statistical analysis foundation

### Long Term (6+ months)
- [ ] Complete experiment effect analysis
- [ ] Multi-environment configuration management
- [ ] Automated experiment decision making
- [ ] SDK ecosystem development

## Contributing

Welcome to submit Issues and Pull Requests!

Development environment requirements:
- Rust 1.70+
- Cargo

Code standards:
```bash
# Format
make fmt

# Lint check
make lint

# Run tests
make test

# Benchmark
make bench
```

## License

MIT License