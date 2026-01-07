# Experiment Data Plane

高性能 Rust 实验系统数据面服务，支持分层实验配置、热更新、原子替换、确定性参数合并和**规则引擎**。

## 核心特性

### 1. Layer 管理
- **分层实验配置**：每个 Layer 独立管理流量分配和参数配置
- **10000 个哈希槽**：提供 0.01% 粒度的流量分配精度
- **版本控制**：支持 Layer 版本管理，便于回滚和灰度发布
- **热更新**：监听配置文件变化，自动加载新配置（无需重启）
- **原子替换**：使用 Arc-Swap 保证配置更新的原子性和无锁读取
- **Salt 机制**：每层使用独立 salt 避免有偏分布

### 2. 规则引擎 ⭐ NEW
- **结构化规则**：基于 JSON 树结构的规则定义（无需 DSL）
- **类型安全**：支持 string、int、float、bool、semver 字段类型
- **丰富的操作符**：比较（eq/neq/gt/gte/lt/lte）、集合（in/not_in）、模式（like/not_like）、布尔（and/or/not）
- **条件分流**：基于用户上下文动态决定实验组匹配
- **向后兼容**：规则可选，不影响现有实验

### 3. 实验请求处理
- **哈希分桶**：基于请求的 hash_key 计算哈希值，映射到实验组
- **规则评估**：在参数合并前评估规则，决定是否匹配
- **多层合并**：按优先级合并多个 Layer 的参数配置
- **确定性合并**：保证相同请求在不同节点得到一致结果
- **嵌套参数合并**：支持 JSON 对象的递归合并
- **Service 强约束**：只返回匹配服务的实验配置

### 4. 性能优化
- **无锁读取**：使用 ArcSwap 实现高并发读取
- **零拷贝**：Arc 共享配置数据，避免不必要的拷贝
- **高效哈希**：使用 XXH3 算法，性能优异且分布均匀
- **规则短路**：布尔操作符短路求值
- **增量更新**：只更新变化的 Layer，不影响其他配置

### 5. 可观测性
- **结构化日志**：基于 tracing 的分级日志
- **Prometheus Metrics**：提供请求量、延迟、错误率等指标
- **健康检查**：提供 `/health` 端点
- **Layer 管理 API**：查询、回滚等运维接口
- **Field Types API**：管理规则字段类型

## 快速开始

### 编译

```bash
cd data_plane
cargo build --release
```

### 运行

```bash
# 复制配置文件
cp .env.example .env

# 启动服务
cargo run --release
```

### Docker 部署

```bash
# 构建镜像
docker build -t experiment-data-plane .

# 运行容器
docker run -d \
  -p 8080:8080 \
  -p 9090:9090 \
  -v $(pwd)/../configs/layers:/configs/layers \
  -e LAYERS_DIR=/configs/layers \
  experiment-data-plane
```

## API 文档

### 查询实验参数

**POST** `/experiment`

请求体：
```json
{
  "service": "ranker_svc",
  "hash_keys": {
    "user_id": "user_12345",
    "session_id": "session_67890"
  },
  "layers": []  // 可选：指定使用的 layers
}
```

响应：
```json
{
  "service": "ranker_svc",
  "parameters": {
    "algorithm": "gbdt",
    "timeout_ms": 150,
    "personalization_enabled": true,
    "model_version": "v2.1"
  },
  "matched_layers": ["personalization_experiment", "click_experiment"]
}
```

### 列出所有 Layers

**GET** `/layers`

响应：
```json
{
  "layers": ["click_experiment", "personalization_experiment", "search_experiment"]
}
```

### 获取 Layer 详情

**GET** `/layers/:layer_id`

响应：
```json
{
  "layer_id": "click_experiment",
  "version": "v1",
  "priority": 100,
  "hash_key": "user_id",
  "buckets": {...},
  "groups": {...},
  "enabled": true
}
```

### 回滚 Layer

**POST** `/layers/:layer_id/rollback`

回滚到上一个版本。

### 字段类型管理 ⭐ NEW

**POST** `/field_types`

更新规则引擎使用的字段类型：

```bash
curl -X POST http://localhost:8080/field_types \
  -H "Content-Type: application/json" \
  -d '{
    "country": "string",
    "age": "int",
    "premium": "bool",
    "app_version": "semver"
  }'
```

**GET** `/field_types`

获取当前字段类型配置。

### 健康检查

**GET** `/health`

### Metrics

**GET** `/metrics`

Prometheus 格式的监控指标。

## Layer 配置格式

### JSON 格式示例

```json
{
  "layer_id": "click_experiment",
  "version": "v1",
  "priority": 100,
  "hash_key": "user_id",
  "salt": "my_custom_salt",
  "enabled": true,
  "buckets": {
    "0": "group_a",
    "1": "group_a",
    "5": "group_b"
  },
  "groups": {
    "group_a": {
      "service": "ranker_svc",
      "params": {
        "algorithm": "lr",
        "timeout_ms": 100
      }
    },
    "group_b": {
      "service": "ranker_svc",
      "params": {
        "algorithm": "gbdt",
        "timeout_ms": 150
      }
    }
  }
}
```

### YAML 格式示例

```yaml
layer_id: personalization_experiment
version: v2
priority: 200
hash_key: user_id
salt: personalization_salt_v2  # 可选，不设置则使用 "layer_id_version"
enabled: true

buckets:
  0: control
  7: treatment

groups:
  control:
    service: ranker_svc
    params:
      personalization_enabled: false
      
  treatment:
    service: ranker_svc
    params:
      personalization_enabled: true
      model_version: v2.1
```

## 配置说明

| 字段 | 说明 | 必填 |
|------|------|------|
| layer_id | Layer 唯一标识 | 是 |
| version | Layer 版本号 | 是 |
| priority | 优先级（越大越优先） | 是 |
| hash_key | 用于哈希的字段名 | 是 |
| salt | 哈希盐值，确保不同层独立分布 | 否（默认为 `{layer_id}_{version}`） |
| enabled | 是否启用 | 否（默认 true） |
| buckets | 桶号到实验组的映射 | 是 |
| groups | 实验组配置 | 是 |

### Salt 的重要性

**为什么需要 Salt？**

在多层实验系统中，如果不同 Layer 使用相同的哈希算法和相同的 hash_key（如都用 user_id），那么同一个用户在不同 Layer 中会被映射到相同的桶号，导致**有偏分布**。

例如：
- Layer A: user_123 → bucket 42
- Layer B: user_123 → bucket 42（相同！）

这意味着：
- 如果 user_123 在 Layer A 中被分到实验组，那在 Layer B 中也更可能被分到实验组
- 不同实验之间失去了独立性
- 无法准确评估实验效果

**Salt 如何解决？**

每个 Layer 使用不同的 salt，确保同一用户在不同 Layer 中有独立的分布：

```rust
// Layer A 使用 salt "click_experiment_v1"
hash("user_123" + "click_experiment_v1") → bucket 42

// Layer B 使用 salt "color_experiment_v1"  
hash("user_123" + "color_experiment_v1") → bucket 7839
```

**最佳实践：**

1. **显式指定 salt**：为每个 Layer 设置有意义的 salt
   ```json
   {
     "layer_id": "click_experiment",
     "version": "v1",
     "salt": "click_exp_2024_q1"
   }
   ```

2. **使用默认 salt**：不指定时，系统自动使用 `{layer_id}_{version}`
   - 优点：简单，版本更新自动改变 salt
   - 缺点：版本更新会导致所有用户重新分配

3. **保持 salt 稳定**：除非需要重新分配流量，否则不要修改 salt

4. **多版本实验**：如果要对比同一用户在不同版本的表现，使用相同的 salt；否则使用不同的 salt

## 参数合并规则

多个 Layer 的参数按以下规则合并：

1. **优先级排序**：按 priority 从高到低处理
2. **Service 过滤**：只处理 service 匹配的 Layer
3. **嵌套对象合并**：递归合并 JSON 对象
4. **标量/数组覆盖**：高优先级 Layer 的值优先
5. **确定性保证**：相同优先级按 layer_id 字典序

### 合并示例

**Layer 1（优先级 200）**：
```json
{
  "timeout": 100,
  "config": {"a": 1, "b": 2}
}
```

**Layer 2（优先级 100）**：
```json
{
  "timeout": 200,
  "config": {"b": 3, "c": 4},
  "extra": "value"
}
```

**合并结果**：
```json
{
  "timeout": 100,           // Layer 1 优先
  "config": {
    "a": 1,                 // Layer 1
    "b": 2,                 // Layer 1 优先
    "c": 4                  // Layer 2
  },
  "extra": "value"          // Layer 2
}
```

## 部署模式

### Sidecar 模式

每个应用实例部署一个数据面实例，通过 localhost 访问：

```
┌─────────────────┐
│  Application    │
│     Process     │
│        ↓        │
│   localhost:8080│
│        ↓        │
│  Data Plane     │
│    Sidecar      │
└─────────────────┘
```

优点：
- 低延迟（本地调用）
- 故障隔离
- 独立扩缩容

### 独立部署模式

数据面作为独立服务部署：

```
┌──────────┐     ┌──────────┐
│  App 1   │────▶│          │
├──────────┤     │  Data    │
│  App 2   │────▶│  Plane   │
├──────────┤     │  Service │
│  App 3   │────▶│          │
└──────────┘     └──────────┘
```

优点：
- 集中管理
- 资源共享
- 简化部署

## 运维指南

### 新增实验

1. 在 `configs/layers/` 目录创建新的 Layer 文件
2. 数据面自动检测并加载（约 100ms 延迟）
3. 查询日志确认加载成功

### 更新实验

1. 修改对应的 Layer 文件
2. 数据面自动热更新
3. 旧版本保存在回滚历史中

### 回滚实验

```bash
curl -X POST http://localhost:8080/layers/click_experiment/rollback
```

### 监控指标

访问 `http://localhost:9090/metrics` 查看：

- `experiment_requests_total`：请求总数
- `experiment_request_errors_total`：错误总数
- `experiment_request_duration_seconds`：请求延迟
- `experiment_layer_reload_total`：Layer 重载次数
- `experiment_active_layers`：活跃 Layer 数量

## 测试

### 单元测试

```bash
cargo test
```

### 集成测试

```bash
cargo test --test integration_test
```

### 性能测试

```bash
cargo bench
```

## 性能指标

- **P50 延迟**：< 1ms
- **P99 延迟**：< 5ms
- **吞吐量**：> 100K QPS（单核）
- **热更新延迟**：< 100ms
- **内存占用**：< 50MB（10 个 Layer）

## 最佳实践

1. **Layer 数量**：建议不超过 20 个活跃 Layer
2. **优先级分配**：预留充足的优先级空间（如 100、200、300）
3. **Service 约束**：始终设置正确的 service 字段
4. **参数大小**：单个 Layer 参数建议 < 1KB
5. **Hash Key**：选择分布均匀的字段（如 user_id）
6. **版本号**：使用语义化版本（如 v1.0.0）
7. **Salt 设置**：为每个 Layer 显式指定独立的 salt
8. **规则引擎**：优先定义字段类型，避免规则过于复杂

## 规则引擎使用 ⭐ NEW

规则引擎允许在实验组级别添加条件判断，基于用户上下文动态决定是否匹配。

### 规则引擎架构

**核心组件**：
1. **Rule Nodes**: 结构化树形规则表示
2. **Field Types**: 来自控制面的类型信息用于验证
3. **Rule Evaluation**: 在 layer merge 过程中基于上下文评估规则
4. **Integration**: 与现有 Layer/Group/Merge 逻辑无缝集成

**设计原则**：
- **轻量级**: 无需 DSL 解析，使用结构化 JSON
- **类型安全**: 字段类型针对控制面元数据验证
- **可组合**: 布尔操作符（AND/OR/NOT）支持嵌套规则
- **向后兼容**: 规则可选，现有 layer 无需更改

### 支持的操作符

**比较操作符**：
- `eq`: 等于
- `neq`: 不等于
- `gt`: 大于
- `gte`: 大于等于
- `lt`: 小于
- `lte`: 小于等于

**集合操作符**：
- `in`: 在列表中
- `not_in`: 不在列表中

**字符串操作符**：
- `like`: 模式匹配（支持 `*` 通配符）
- `not_like`: 否定模式匹配

**布尔操作符**：
- `and`: 所有子节点为真
- `or`: 至少一个子节点为真
- `not`: 否定子节点结果

### 字段类型

支持的字段类型：
- `string`: 文本值
- `int`: 整数
- `float`: 浮点数
- `bool`: 布尔值（true/false）
- `semver`: 语义化版本（如 "1.2.3"）

### 快速开始

**步骤 1：配置字段类型**

```bash
curl -X POST http://localhost:8080/field_types \
  -H "Content-Type: application/json" \
  -d '{
    "country": "string",
    "age": "int",
    "premium": "bool",
    "app_version": "semver"
  }'
```

**步骤 2：在 Layer 中添加规则**

```json
{
  "layer_id": "us_adult_promo",
  "version": "v1",
  "priority": 100,
  "hash_key": "user_id",
  "enabled": true,
  "buckets": {
    "0": "control",
    "5000": "treatment"
  },
  "groups": {
    "treatment": {
      "service": "promo",
      "params": {"discount": 0.15},
      "rule": {
        "type": "and",
        "children": [
          {
            "type": "field",
            "field": "country",
            "op": "eq",
            "values": ["US"]
          },
          {
            "type": "field",
            "field": "age",
            "op": "gte",
            "values": [18]
          }
        ]
      }
    }
  }
}
```

**步骤 3：发送带上下文的请求**

```bash
curl -X POST http://localhost:8080/experiment \
  -H "Content-Type: application/json" \
  -d '{
    "service": "promo",
    "hash_keys": {"user_id": "user_12345"},
    "context": {
      "country": "US",
      "age": 25
    }
  }'
```

### 常用规则模式

**模式 1：国家/地域定位**
```json
{
  "type": "field",
  "field": "country",
  "op": "in",
  "values": ["US", "CA", "UK"]
}
```

**模式 2：年龄门槛**
```json
{
  "type": "field",
  "field": "age",
  "op": "gte",
  "values": [18]
}
```

**模式 3：会员用户**
```json
{
  "type": "field",
  "field": "premium",
  "op": "eq",
  "values": [true]
}
```

**模式 4：版本检查**
```json
{
  "type": "field",
  "field": "app_version",
  "op": "gte",
  "values": ["2.0.0"]
}
```

**模式 5：模式匹配**
```json
{
  "type": "field",
  "field": "email",
  "op": "like",
  "values": ["*@company.com"]
}
```

**模式 6：复杂 AND/OR 组合**
```json
{
  "type": "and",
  "children": [
    {
      "type": "or",
      "children": [
        {"type": "field", "field": "country", "op": "eq", "values": ["US"]},
        {"type": "field", "field": "country", "op": "eq", "values": ["CA"]}
      ]
    },
    {
      "type": "or",
      "children": [
        {"type": "field", "field": "age", "op": "gte", "values": [18]},
        {"type": "field", "field": "premium", "op": "eq", "values": [true]}
      ]
    }
  ]
}
```

**模式 7：NOT 排除**
```json
{
  "type": "not",
  "child": {
    "type": "field",
    "field": "country",
    "op": "eq",
    "values": ["US"]
  }
}
```

### 规则评估流程

1. **请求到达**，包含 service、hash_keys 和 context
2. **Layer 按优先级排序**（从高到低）
3. **对每个 layer**：
   - 使用 hash_key + salt 计算桶号
   - 获取桶对应的组
   - 检查 service 约束
   - **评估规则**（如果存在）针对 context
   - 如果规则通过，合并组参数
4. **返回合并后的参数**

### 错误处理

规则失败时优雅降级并记录日志：
- **字段未找到**：记录警告，跳过该组
- **类型不匹配**：记录警告，跳过该组
- **无效操作符**：记录警告，跳过该组
- **缺少上下文**：记录警告，跳过该组

这确保规则错误不会破坏整个请求。

### 规则验证

规则在加载时验证：
- 字段名必须存在于 field_types 映射中
- 值必须匹配声明的字段类型
- 操作符必须对节点类型有效
- 布尔节点的 children 数组不能为空

### 性能考虑

- **轻量级**：规则预解析为 JSON，无运行时 DSL 解析
- **早期退出**：布尔操作符短路求值（AND 遇到 false 停止，OR 遇到 true 停止）
- **只读**：字段类型缓存在内存中（Arc<RwLock>）
- **评估期间无锁**：规则评估是纯函数，不需要锁

### 规则引擎最佳实践

1. **早期定义字段类型**：在创建规则前配置所有字段
2. **保持规则简单**：使用多个 layer 而不是过度复杂的规则
3. **测试两条路径**：验证规则通过和失败的情况
4. **使用一致的命名**：字段名在控制面和客户端代码中保持一致
5. **处理缺失上下文**：如果上下文字段缺失，规则评估失败（跳过组）
6. **监控规则失败**：检查日志中的规则评估错误

### 规则引擎变更日志

**核心功能**：
- Rule Engine 模块（`src/rule.rs`）
  - `FieldType` 枚举：string, int, float, bool, semver
  - `Op` 枚举：13 个操作符
  - `Node` 枚举：结构化树形规则表示
  - `Node::validate()`: 类型安全的规则验证
  - `Node::evaluate()`: 基于上下文的规则评估

**Layer 集成**：
- 更新 `Group` 结构（`src/layer.rs`）
  - 为 groups 添加可选的 `rule` 字段
  - 向后兼容（None = 总是匹配）

**Merge 逻辑增强**：
- 增强 `merge_layers()`（`src/merge.rs`）
  - 添加 `field_types` 参数用于规则验证
  - 在 service 检查后、参数合并前评估规则
  - 优雅的错误处理（记录警告，继续）
  - 更新 `ExperimentRequest` 添加 `context` 字段

**Server API**：
- 新端点（`src/server.rs`）
  - `POST /field_types`: 更新字段类型映射
  - `GET /field_types`: 获取当前字段类型

**测试覆盖**：
- 26 个规则单元测试
- 6 个规则集成测试
- 所有现有测试已更新以适配新 API
- 总计 69 个测试全部通过 ✅

### 示例配置文件

查看 `configs/layers/` 目录：
- `us_adult_experiment.json` - 年龄门槛 + 国家定位
- `premium_feature_rollout.json` - 会员用户 + 版本检查
- `regional_experiment.json` - 多地域 IN 操作符 + NOT

## 故障排查

### Layer 加载失败

检查日志中的错误信息：
```bash
grep "Failed to load layer" logs/data-plane.log
```

常见原因：
- JSON/YAML 格式错误
- bucket 引用的 group 不存在
- bucket 编号超出范围（>= 10000）

### 参数未生效

1. 确认 service 字段匹配
2. 检查 hash_key 是否传递
3. 验证 bucket 映射是否正确
4. 查看 matched_layers 判断哪些 Layer 被匹配

### 性能问题

1. 检查 Layer 数量是否过多
2. 查看参数大小是否过大
3. 监控 metrics 中的延迟分布
4. 考虑使用 Sidecar 模式降低网络延迟

## 架构设计

### 核心组件

```
┌──────────────────────────────────────┐
│          HTTP Server (Axum)          │
└─────────────┬────────────────────────┘
              │
┌─────────────▼────────────────────────┐
│        Merge Engine                  │
│  - Priority Sorting                  │
│  - Hash Calculation                  │
│  - Deterministic Merge               │
└─────────────┬────────────────────────┘
              │
┌─────────────▼────────────────────────┐
│       Layer Manager                  │
│  - ArcSwap (Lock-free)               │
│  - Version Control                   │
│  - Rollback History                  │
└─────────────┬────────────────────────┘
              │
┌─────────────▼────────────────────────┐
│       File Watcher                   │
│  - notify (inotify/FSEvents)         │
│  - Debounce                          │
└──────────────────────────────────────┘
```

### 并发模型

- **读操作**：无锁，使用 ArcSwap
- **写操作**：仅在热更新时加锁，不影响读取
- **异步 IO**：基于 Tokio 异步运行时

## 后续优化方向

- [ ] gRPC 支持
- [ ] 分布式配置中心集成（如 etcd）
- [ ] A/B 测试统计分析
- [ ] 流量回放和模拟
- [ ] 更细粒度的指标（如按 Layer 统计）
- [ ] 配置验证和 Dry-run 模式
- [ ] Web UI 管理界面
- [x] **规则引擎** ⭐ 已完成
  - 结构化规则定义
  - 类型安全的字段验证
  - 丰富的操作符（比较/集合/模式/布尔）
  - 与 Layer merge 无缝集成

## License

MIT
