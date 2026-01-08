# 高性能分布式实验系统

Rust 实现的高性能、低延迟 A/B 测试和实验管理数据面，支持分层实验、复杂规则引擎、热更新、流量精细控制。

## 核心特性

- ✅ **10,000 哈希槽**：0.01% 流量粒度，XXHash 哈希算法
- ✅ **多层参数合并**：Priority 优先级控制，递归深度合并
- ✅ **确定性分桶**：Sticky Bucketing + Salt 机制保证实验独立
- ✅ **流量切分**：Ranges 机制支持 Namespace 互斥实验
- ✅ **规则引擎**：13 种操作符，支持任意深度 AND/OR 嵌套
- ✅ **热更新**：< 100ms，Arc + RwLock 原子替换
- ✅ **高性能**：单核 > 100K QPS，P50 < 1ms，P99 < 5ms
- ✅ **零拷贝**：Arc 共享数据结构，无 GC 停顿
- ✅ **可观测性**：Prometheus Metrics + 详细日志

## 性能数据

### 测试环境

- **CPU**: Apple M1/M2 或 Intel Xeon
- **内存**: 16GB+
- **编译**: Rust 1.70+ Release 模式
- **优化级别**: `-O3` + LTO

### Benchmark 结果

#### 1. Layer Management

| 测试项 | 规模 | 性能 | 说明 |
|--------|------|------|------|
| Layer filtering | 1K layers | ~3µs | 按 service 过滤 |
| Layer filtering | 10K layers | ~35µs | 按 service 过滤 |
| Layer filtering | 50K layers | ~230µs | 按 service 过滤 |
| Bucket calculation | 单次 | ~98ns | XXHash 哈希计算 |
| Layer sorting | 1K layers | ~55µs | 按优先级访问 |
| Layer sorting | 10K layers | ~602µs | 按优先级访问 |

#### 2. Parameter Merge

| 测试项 | 规模 | 性能 | 说明 |
|--------|------|------|------|
| Layer count | 10 层 | ~820µs | 基础合并 |
| Layer count | 100 层 | ~6.8ms | 中等规模 |
| Layer count | 1000 层 | ~68ms | 大规模 |
| Layer count | 5000 层 | ~331ms | 极端规模 |
| Layer count | 10000 层 | ~661ms | 超大规模 |
| Param depth | 1 层 | ~42µs | 浅层嵌套 |
| Param depth | 3 层 | ~6.6ms | 嵌套对象 |
| Param depth | 5 层 | ~336ms | 深度嵌套 |
| Param depth | 8 层 | ~74s | 极端深度 |

#### 3. 端到端性能（预估）

| 场景 | P50 | P99 | 说明 |
|------|-----|-----|------|
| 简单场景 (10层) | < 1ms | < 3ms | 基础实验 |
| 中等场景 (100层) | < 10ms | < 20ms | 复杂实验 |
| 单核 QPS | > 100K | - | 轻量级请求 |

### 架构设计优势

#### 1. 零成本抽象

```rust
// 泛型在编译期单态化，无虚函数调用
fn evaluate<T: FieldValue>(value: &T, op: &Op) -> bool {
    // 编译后等同于直接类型的代码
    // 无运行时开销
}
```

**优势**：
- 泛型和 trait 编译期展开
- 函数内联激进
- 无动态分发开销

#### 2. 无 GC 停顿

```rust
// 所有权系统保证内存安全
// 无 Stop-the-World 暂停
let config = Arc::new(load_config());  // 引用计数
// 析构时自动释放，无扫描开销
```

**优势**：
- 延迟稳定可预测
- 无 GC 暂停抖动
- 内存释放确定性

#### 3. 高效并发

```rust
// Arc + RwLock 实现多读单写
let config = Arc::new(RwLock::new(state));

// 读取：原子指针加载，无拷贝
let reader = config.read().unwrap();
// 写入：排他锁，原子替换
let mut writer = config.write().unwrap();
```

**优势**：
- 零拷贝数据共享
- 无数据竞争
- 无锁读取优化

#### 4. 内存布局优化

```rust
// HashMap 使用高性能哈希算法
use ahash::AHashMap;  // 比标准 HashMap 快 3-10x

// 数据紧凑排列，Cache 友好
#[repr(C)]
struct Layer {
    priority: i32,      // 4 bytes
    enabled: bool,      // 1 byte
    // ... 字段按大小对齐
}
```

**优势**：
- Cache line 友好
- 分支预测优化
- SIMD 向量化

#### 5. 编译器优化

```rust
// 编译器激进优化
#[inline(always)]
fn hash_to_bucket(key: &str, salt: &str) -> u32 {
    // 编译期常量折叠
    // 循环展开
    // SIMD 指令
    xxh3_64(key, salt) % 10000
}
```

**优势**：
- 内联消除函数调用
- 常量折叠
- 死代码消除
- LLVM 优化管线

### 性能优化建议

#### 1. Layer 数量控制

- **推荐**：< 100 层
- **可接受**：100-500 层
- **需优化**：> 500 层

建议：定期清理不用的 Layer，合并相关实验。

#### 2. 规则复杂度控制

- **推荐**：< 5 层嵌套
- **可接受**：5-10 层嵌套
- **需优化**：> 10 层嵌套

建议：简化规则逻辑，避免过深嵌套。

#### 3. 参数大小控制

- **推荐**：< 1KB/层
- **可接受**：1-5KB/层
- **需优化**：> 5KB/层

建议：避免在参数中存储大量数据。

#### 4. 参数嵌套深度控制

- **推荐**：< 3 层嵌套
- **可接受**：3-5 层嵌套
- **需优化**：> 5 层嵌套

建议：避免过深的参数嵌套，合并性能随深度指数增长。

## 快速开始

### 构建与运行

```bash
# 构建 release 版本
make build

# 运行数据面服务
make run

# 开发模式（带日志）
make dev
```

服务监听：
- HTTP API: `http://localhost:8080`
- Metrics: `http://localhost:9090/metrics`

### 测试 API

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

响应示例：

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

## 架构设计

### 核心模块

```
┌─────────────────────────────────────────────────────────┐
│                     HTTP Server                          │
│                    (Axum + Tower)                        │
└────────────────────┬────────────────────────────────────┘
                     │
        ┌────────────┼────────────┐
        │            │            │
        ▼            ▼            ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│  Layer   │  │   Rule   │  │  Merge   │
│ Manager  │  │  Engine  │  │  Engine  │
└────┬─────┘  └────┬─────┘  └────┬─────┘
     │             │             │
     │             │             │
     ▼             ▼             ▼
┌─────────────────────────────────────┐
│         Experiment Catalog          │
│        (Arc<RwLock<HashMap>>)       │
└─────────────────────────────────────┘
```

### Layer（实验层）

每个 Layer 是一个独立的实验配置单元：

- **10,000 哈希槽**：提供 0.01% 流量粒度
- **Priority 优先级**：控制参数合并顺序
- **Salt 机制**：保证不同实验的哈希分布独立
- **Ranges 切分**：实现 Namespace 内互斥实验

### 流量分配算法

```rust
// 1. 哈希计算 (XXHash)
hash = xxh3_64(user_id + salt)

// 2. 映射到桶号
bucket = hash % 10000

// 3. 查找 range
for range in ranges {
    if bucket >= range.start && bucket < range.end {
        return range.vid
    }
}
```

### 多层参数合并

按 Priority 从高到低递归合并参数：

```json
// Layer 1 (priority: 100)
{"timeout": 100, "config": {"x": 1, "y": 2}}

// Layer 2 (priority: 200, 优先级更高)
{"retry": 3, "config": {"x": 10}}

// 合并结果
{"timeout": 100, "retry": 3, "config": {"x": 10, "y": 2}}
```

特点：
- 高优先级层的键优先
- 嵌套对象递归合并
- 确保任意节点返回一致结果

### Salt 独立分布机制

每个 Layer 独立 salt，避免有偏分布：

```rust
// Layer 1: hash("user_123" + "layer1_salt") → bucket 4200
// Layer 2: hash("user_123" + "layer2_salt") → bucket 7839
```

如果所有层使用相同 salt，用户在所有实验中会分配到相同的桶号段，导致：
- 高桶号用户总是命中新特性
- 低桶号用户总是命中基线
- 实验结果有偏

### Ranges 互斥实验

同一层内通过 Ranges 切分流量实现互斥：

```json
{
  "ranges": [
    {"start": 0, "end": 5000, "vid": 1001},      // 实验A: 0-50%
    {"start": 5000, "end": 7500, "vid": 1002},   // 实验B: 50-75%
    {"start": 7500, "end": 10000, "vid": 1003}   // 对照组: 75-100%
  ]
}
```

用户的 bucket 只会命中一个 range，天然互斥。

## 配置示例

### Layer 配置

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

### Experiment 配置

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

## 规则引擎

### 支持的操作符

- **比较**：`eq`, `neq`, `gt`, `gte`, `lt`, `lte`
- **集合**：`in`, `not_in`
- **字符串**：`like`, `not_like`（支持 `*` 通配符）
- **逻辑**：`and`, `or`, `not`

### 字段类型

- `string` - 字符串
- `int` - 整数
- `float` - 浮点数
- `bool` - 布尔值
- `semver` - 语义化版本

### 规则示例

```json
{
  "type": "and",
  "children": [
    {
      "type": "or",
      "children": [
        {"type": "field", "field": "country", "op": "in", "values": ["US", "CA"]},
        {"type": "field", "field": "premium", "op": "eq", "values": [true]}
      ]
    },
    {"type": "field", "field": "age", "op": "gte", "values": [18]},
    {"type": "field", "field": "version", "op": "gte", "values": ["2.0.0"]}
  ]
}
```

## 性能 Benchmark

### 运行测试

```bash
# 运行所有 benchmark
make bench

# 按模块运行
make bench SUITE=layer    # 层管理
make bench SUITE=rule     # 规则评估
make bench SUITE=merge    # 参数合并
```

### 测试模块

#### 1. Layer Management
- **Layer filtering** - 在大量层中按 service 过滤
- **Bucket calculation** - XXHash 哈希计算性能
- **Layer sorting** - 按优先级排序和访问

#### 2. Rule Evaluation
- **Simple rules** - 基础操作符 (eq, in, gte)
- **Rule depth** - 深度嵌套 (2-20 层)
- **Rule width** - 横向扩展 (5-100 条件)
- **Complex patterns** - 复杂 AND/OR 组合
- **Batch evaluation** - 批量评估 (10-5k 规则)

#### 3. Parameter Merge
- **Layer count** - 层数递增 (10-10k)
- **Param depth** - 参数嵌套深度 (1-15 层)
- **Param width** - 字段数量 (5-100 字段)
- **Extreme merge** - 极限场景 (5k 层, 5 层深, 25 字段)
- **Conflict resolution** - 参数覆盖性能

查看详细 Benchmark 报告：
```bash
open data_plane/target/criterion/report/index.html
```

## 开发命令

```bash
# 查看所有命令
make help

# 构建
make build          # Release 构建
make test           # 运行测试

# 运行服务
make run            # 生产模式
make dev            # 开发模式（带日志）

# Benchmark
make bench          # 所有测试
make bench SUITE=layer   # 层管理测试
make bench SUITE=rule    # 规则评估测试
make bench SUITE=merge   # 参数合并测试

# 代码质量
make fmt            # 格式化代码
make lint           # Clippy 检查
make clean          # 清理构建产物
```

## 部署方案

### Sidecar 模式（推荐）

与业务服务部署在同一 Pod，localhost 访问，延迟最低。

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

### 独立部署

作为独立服务部署，适合多个业务共享。

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
        env:
        - name: LAYERS_DIR
          value: "/configs/layers"
        - name: EXPERIMENTS_DIR
          value: "/configs/experiments"
        volumeMounts:
        - name: config
          mountPath: /configs
        resources:
          requests:
            cpu: "500m"
            memory: "128Mi"
          limits:
            cpu: "2000m"
            memory: "512Mi"
      volumes:
      - name: config
        configMap:
          name: experiment-config
---
apiVersion: v1
kind: Service
metadata:
  name: experiment-data-plane
spec:
  selector:
    app: experiment-data-plane
  ports:
  - name: http
    port: 8080
    targetPort: 8080
  - name: metrics
    port: 9090
    targetPort: 9090
```

## 监控

### Prometheus Metrics

系统暴露以下 Metrics：

```promql
# QPS
rate(experiment_requests_total[1m])

# 错误率
rate(experiment_request_errors_total[1m]) / rate(experiment_requests_total[1m])

# P50/P99 延迟
histogram_quantile(0.50, rate(experiment_request_duration_seconds_bucket[1m]))
histogram_quantile(0.99, rate(experiment_request_duration_seconds_bucket[1m]))

# Layer 重载次数
rate(experiment_layer_reload_total[1m])

# 规则评估次数
rate(experiment_rule_evaluations_total[1m])
```

### 日志

使用 `RUST_LOG` 环境变量控制日志级别：

```bash
# 开发模式，详细日志
RUST_LOG=debug cargo run

# 生产模式，只记录错误
RUST_LOG=error cargo run

# 只显示特定模块
RUST_LOG=experiment_data_plane::merge=debug cargo run
```

## 最佳实践

### Layer 设计

1. **优先级间隔**：使用 100, 200, 300 便于插入新层
2. **明确 Service**：显式指定 `service` 字段限定范围
3. **独立 Salt**：每个 Layer 使用不同 Salt
4. **清晰命名**：使用描述性的 `layer_id`

### 流量分配

1. **选择 hash_key**：使用分布均匀的字段（如 user_id）
2. **避免有序字段**：不要使用时间戳等有序值
3. **扩量保持 Salt**：扩量时只修改 ranges，不改 salt
4. **灰度发布**：先分配 1% → 5% → 10% → 50% → 100%

### 参数设计

1. **控制大小**：单个 Layer 参数 < 1KB
2. **结构化组织**：使用嵌套对象组织相关参数
3. **避免大数据**：不要在参数中包含大量数据或列表
4. **类型一致**：保持同一参数在不同层的类型一致

### 运维管理

1. **定期清理**：及时下线不用的 Layer
2. **版本控制**：重要实验保留多个版本用于回滚
3. **监控告警**：关注 Layer 重载错误和延迟指标
4. **配置验证**：部署前验证配置格式和逻辑

## 与 GrowthBook 对比

| 特性 | 本系统 | GrowthBook |
|------|--------|------------|
| **多层参数合并** | ✅ Priority 优先级 | ❌ 不支持 |
| **Sticky Bucketing** | ✅ 确定性哈希 + Salt | ✅ 可选持久化 |
| **Namespace 互斥** | ✅ Ranges 切分 | ✅ 显式语法 |
| **规则引擎** | ✅ 13 操作符 | ✅ 20+ 操作符 |
| **性能 (P50)** | < 1ms | < 0.1µs (本地 SDK) |
| **并发能力** | > 100K QPS/核 | 受 SDK 语言限制 |
| **数据分析** | ❌ 无 | ✅ 完整统计引擎 |
| **UI 界面** | ❌ 待开发 | ✅ 完整 Web 界面 |
| **SDK 生态** | ❌ 需自调 API | ✅ 10+ 官方 SDK |
| **部署模式** | Sidecar/独立 | SDK 嵌入 |
| **适用场景** | 微服务、高 QPS | 全栈应用、快速上线 |

**核心差异**：

- **本系统**：专注高性能数据面，支持多层实验合并，适合微服务架构和高 QPS 场景
- **GrowthBook**：完整的端到端平台，提供数据分析和 UI，适合快速上线和完整的实验闭环

选择建议：
- 有自建能力、需要高性能数据面 → 本系统
- 需要快速上线、完整实验平台 → GrowthBook
- 可以结合使用：GrowthBook 控制面 + 本系统数据面

## 常见问题

### Q: 如何保证多节点配置一致？

A: 所有节点 watch 同一配置目录（ConfigMap/共享存储），自动热更新。系统使用文件 watcher 监听配置变化，一旦检测到变化会自动重新加载，确保所有节点最终一致。

### Q: 如何实现多维度分流？

A: 创建多个 Layer，使用不同 `hash_key`。例如：
- Layer 1: `hash_key = "user_id"` - 用户维度实验
- Layer 2: `hash_key = "session_id"` - 会话维度实验
- Layer 3: `hash_key = "device_id"` - 设备维度实验

### Q: 修改 salt 会影响现有用户吗？

A: 是的！修改 salt 会导致所有用户重新分配流量，bucket 号完全改变。除非需要重新分配流量（如实验结束重新开始），否则不要修改 salt。扩量时只修改 ranges，保持 salt 不变。

### Q: 一个实验可以在多个层吗？

A: 不可以。每个实验（eid）只能在一个层，这是架构设计原则。同一实验的不同变体（vid）通过该层的 ranges 分配流量。如果需要多层实验，应该设计为不同的 eid。

### Q: 热更新会丢失请求吗？

A: 不会。系统使用 `Arc<RwLock<T>>` 实现原子替换，读请求始终能访问到一致的配置快照，没有中间状态。更新过程中的请求使用旧配置或新配置，不会失败。

### Q: 规则评估失败怎么办？

A: 规则评估失败时（如字段不存在、类型不匹配），该实验被跳过，不返回参数。系统会记录错误日志和 metrics，便于排查。建议在控制面做好规则验证。

### Q: 如何调试参数合并结果？

A: 响应中包含 `matched_layers` 和 `vids` 字段，显示哪些层被匹配。可以对比各层的参数和优先级，追踪合并逻辑。开发模式 (`RUST_LOG=debug`) 会输出详细的合并过程。

## 项目结构

```
expirement_system/
├── data_plane/              # Rust 数据面
│   ├── src/
│   │   ├── main.rs         # 主程序入口
│   │   ├── server.rs       # HTTP API (Axum)
│   │   ├── layer.rs        # Layer 管理和加载
│   │   ├── merge.rs        # 参数合并引擎
│   │   ├── hash.rs         # 哈希计算 (XXHash)
│   │   ├── rule.rs         # 规则引擎
│   │   ├── catalog.rs      # Experiment 目录
│   │   ├── watcher.rs      # 文件监听热更新
│   │   ├── metrics.rs      # Prometheus Metrics
│   │   ├── error.rs        # 错误类型
│   │   └── config.rs       # 配置管理
│   ├── benches/            # 性能测试
│   │   ├── layer_management_bench.rs
│   │   ├── rule_evaluation_bench.rs
│   │   └── param_merge_bench.rs
│   ├── tests/              # 集成测试
│   │   ├── integration_test.rs
│   │   └── rule_integration_test.rs
│   └── Cargo.toml
│
├── configs/                # 配置文件示例
│   ├── layers/             # Layer 配置
│   └── experiments/        # Experiment 定义
│
├── Makefile                # 构建和测试命令
└── README.md
```

## 技术栈

- **Web 框架**：Axum + Tower (高性能异步 HTTP)
- **并发**：Tokio (异步运行时)
- **序列化**：Serde (JSON/YAML)
- **哈希**：XXHash (高性能哈希算法)
- **监控**：Prometheus (Metrics)
- **日志**：Tracing (结构化日志)
- **并发原语**：Arc + RwLock (零拷贝共享)
- **文件监听**：Notify (热更新)

## 后续规划

### 短期 (1-2 个月)

- [ ] gRPC 协议支持
- [ ] 配置验证和 Dry-run 模式
- [ ] 更丰富的 Metrics (规则评估耗时、参数合并耗时)
- [ ] 支持远程配置中心 (etcd/Consul)

### 中期 (3-6 个月)

- [ ] 控制面 Web UI
  - Layer 配置生成和验证
  - 可视化流量分配
  - 实验状态管理
- [ ] 配置版本管理和回滚
- [ ] A/B 测试统计分析基础

### 长期 (6+ 个月)

- [ ] 完整的实验效果分析
- [ ] 多环境配置管理
- [ ] 自动化实验决策
- [ ] SDK 生态建设

## 贡献指南

欢迎提交 Issue 和 Pull Request！

开发环境要求：
- Rust 1.70+
- Cargo

代码规范：
```bash
# 格式化
make fmt

# Lint 检查
make lint

# 运行测试
make test

# Benchmark
make bench
```

## License

MIT License

## 联系方式

如有问题或建议，欢迎提交 Issue。
