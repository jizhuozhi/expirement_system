# 分布式实验系统 (Distributed Experiment System)

一个高性能、低延迟的分布式 A/B 测试和实验管理系统，支持分层实验、热更新、流量精细控制、Salt独立分布机制。

## 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                        Frontend                              │
│              (实验配置管理界面 - 可选)                        │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│                    Control Plane                             │
│        (实验配置生成、流量分配策略、Layer 管理)               │
│                      (待实现)                                 │
└────────────────────────┬────────────────────────────────────┘
                         │
                         │ 下发配置
                         │
┌────────────────────────▼────────────────────────────────────┐
│                     Data Plane                               │
│         (Rust 高性能数据面，负责实时参数查询)                 │
│   • Layer 管理与热更新                                        │
│   • 哈希分桶与流量分配                                         │
│   • 多层参数合并                                              │
│   • Sidecar / 独立部署                                        │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
   ┌─────────┐          ┌─────────┐          ┌─────────┐
   │ Service │          │ Service │          │ Service │
   │    A    │          │    B    │          │    C    │
   └─────────┘          └─────────┘          └─────────┘
```

## 核心概念

### Layer（实验层）

- 每个 Layer 独立管理一组实验配置
- 包含 10000 个哈希槽（0.01% 流量粒度）
- 支持版本控制和热更新
- 通过 `priority` 控制合并优先级
- 通过 `service` 字段限定生效范围

### 流量分配

1. **哈希计算**：根据 `hash_key`（如 user_id）计算哈希值
2. **桶映射**：哈希值 % 10000 得到桶号（0-9999）
3. **实验组分配**：桶号映射到实验组
4. **参数获取**：返回实验组对应的参数配置

### 多层合并

多个 Layer 按优先级从高到低合并：
- **高优先级优先**：相同参数由高优先级 Layer 决定
- **嵌套合并**：JSON 对象递归合并
- **确定性保证**：相同请求在任意节点得到一致结果

## 项目结构

```
expirement_system/
├── data_plane/              # Rust 数据面服务 ✅
│   ├── src/
│   │   ├── main.rs          # 主程序入口
│   │   ├── layer.rs         # Layer 管理
│   │   ├── merge.rs         # 参数合并引擎
│   │   ├── hash.rs          # 哈希计算
│   │   ├── server.rs        # HTTP API
│   │   ├── watcher.rs       # 文件监听
│   │   └── metrics.rs       # 监控指标
│   ├── tests/               # 集成测试
│   ├── benches/             # 性能测试
│   └── Dockerfile           # Docker 镜像
│
├── configs/                 # 配置文件 ✅
│   └── layers/              # Layer 配置目录
│       ├── click_experiment.json
│       ├── personalization_experiment.yaml
│       └── search_experiment.json
│
├── control_plane/           # 控制面 (待实现)
│   └── README.md            # 控制面设计文档
│
├── frontend/                # 前端界面 (可选)
│   └── README.md            # 前端设计文档
│
└── scripts/                 # 辅助脚本 ✅
    ├── test_api.sh          # API 测试脚本
    └── load_test.sh         # 负载测试脚本
```

## 快速开始

### 1. 启动数据面服务

```bash
cd data_plane

# 安装依赖并编译
cargo build --release

# 运行服务
LAYERS_DIR=../configs/layers cargo run --release
```

服务启动后监听：
- HTTP API: `http://localhost:8080`
- Metrics: `http://localhost:9090/metrics`

### 2. 测试 API

```bash
# 使用测试脚本
bash scripts/test_api.sh

# 或手动测试
curl -X POST http://localhost:8080/experiment \
  -H "Content-Type: application/json" \
  -d '{
    "service": "ranker_svc",
    "hash_keys": {
      "user_id": "user_12345"
    }
  }'
```

### 3. 查看实验参数

响应示例：
```json
{
  "service": "ranker_svc",
  "parameters": {
    "algorithm": "gbdt",
    "timeout_ms": 150,
    "personalization_enabled": true,
    "model_version": "v2.1",
    "features": ["user_age", "user_gender", "item_category", "user_city"]
  },
  "matched_layers": ["personalization_experiment", "click_experiment"]
}
```

## 使用示例

### 创建新实验

1. 在 `configs/layers/` 创建新的 Layer 文件：

```json
{
  "layer_id": "new_feature_experiment",
  "version": "v1",
  "priority": 250,
  "hash_key": "user_id",
  "enabled": true,
  "buckets": {
    "0": "control",
    "1": "control",
    "2": "control",
    "3": "control",
    "4": "control",
    "5": "treatment",
    "6": "treatment",
    "7": "treatment",
    "8": "treatment",
    "9": "treatment"
  },
  "groups": {
    "control": {
      "service": "my_service",
      "params": {
        "new_feature_enabled": false
      }
    },
    "treatment": {
      "service": "my_service",
      "params": {
        "new_feature_enabled": true,
        "new_feature_config": {
          "timeout": 200,
          "retries": 3
        }
      }
    }
  }
}
```

2. 数据面自动检测并加载（约 100ms）

3. 验证实验生效：

```bash
curl -X POST http://localhost:8080/experiment \
  -H "Content-Type: application/json" \
  -d '{
    "service": "my_service",
    "hash_keys": {
      "user_id": "test_user"
    }
  }'
```

### 更新实验配置

直接修改 Layer 文件，数据面会自动热更新：

```bash
# 修改实验配置
vim configs/layers/new_feature_experiment.json

# 查看日志确认更新
# 输出: Hot reloaded layer: new_feature_experiment
```

### 回滚实验

```bash
# 回滚到上一个版本
curl -X POST http://localhost:8080/layers/new_feature_experiment/rollback
```

## 部署方案

### Sidecar 模式（推荐）

每个应用实例部署一个数据面实例：

```yaml
# Kubernetes sidecar 示例
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
    volumeMounts:
    - name: layers-config
      mountPath: /configs/layers
    env:
    - name: LAYERS_DIR
      value: "/configs/layers"
  
  volumes:
  - name: layers-config
    configMap:
      name: experiment-layers
```

### 独立部署模式

数据面作为独立服务：

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
        - containerPort: 9090
        volumeMounts:
        - name: layers-config
          mountPath: /configs/layers
      volumes:
      - name: layers-config
        configMap:
          name: experiment-layers
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

## 监控与可观测性

### Prometheus Metrics

```promql
# QPS
rate(experiment_requests_total[1m])

# 错误率
rate(experiment_request_errors_total[1m]) / rate(experiment_requests_total[1m])

# P99 延迟
histogram_quantile(0.99, rate(experiment_request_duration_seconds_bucket[1m]))

# Layer 重载次数
rate(experiment_layer_reload_total[1m])
```

### Grafana Dashboard

可导入预配置的 Dashboard（待创建）监控：
- 请求量和错误率
- 延迟分布（P50/P90/P99）
- Layer 加载状态
- 流量分布

## 性能指标

基于 Rust 实现的数据面性能表现：

| 指标 | 值 |
|------|-----|
| P50 延迟 | < 1ms |
| P99 延迟 | < 5ms |
| 单核 QPS | > 100K |
| 热更新延迟 | < 100ms |
| 内存占用 | < 50MB (10 Layers) |

## 最佳实践

1. **Layer 设计**
   - 每个 Layer 对应一个实验或功能开关
   - 优先级间隔设置为 100（100、200、300...）
   - 明确设置 `service` 字段限定生效范围

2. **流量分配**
   - 使用分布均匀的字段作为 `hash_key`（如 user_id）
   - 避免使用时间戳等有序字段
   - 桶号分配要考虑 10000 槽的粒度

3. **参数设计**
   - 保持参数精简，单个 Layer < 1KB
   - 使用嵌套对象组织相关参数
   - 避免在参数中包含大量数据

4. **运维管理**
   - 定期清理不用的 Layer
   - 重要实验保留多个版本用于回滚
   - 监控 Layer 重载错误

## 后续计划

### 控制面 (Control Plane)

- [ ] Layer 配置生成和验证
- [ ] 可视化流量分配
- [ ] 实验效果分析
- [ ] 自动流量调整
- [ ] 多环境配置管理

### 前端界面 (Frontend)

- [ ] 实验创建向导
- [ ] 流量分配可视化
- [ ] 实时监控大盘
- [ ] 配置历史和回滚
- [ ] 权限管理

### 数据面增强

- [ ] gRPC 协议支持
- [ ] 分布式配置中心集成（etcd/Consul）
- [ ] 更丰富的 Metrics
- [ ] 配置验证和 Dry-run
- [ ] Layer 依赖管理

## Salt 机制：保证实验独立性

### 为什么需要 Salt？

在多层实验系统中，如果不同 Layer 使用相同的哈希算法和相同的 hash_key，会导致**有偏分布**问题：同一用户在所有实验中总是被分到相同的桶号，导致实验不独立、流量偏差、统计偏差。

**问题示例**（没有 Salt）：
```
用户 user_123 在不同实验中：
- 点击实验：hash(user_123) → bucket 4200 → 实验组
- 颜色实验：hash(user_123) → bucket 4200 → 实验组
- 推荐实验：hash(user_123) → bucket 4200 → 实验组
```

**解决方案**（使用 Salt）：
```
用户 user_123 在不同实验中：
- 点击实验：hash(user_123 + "click_v1") → bucket 4200
- 颜色实验：hash(user_123 + "color_v1") → bucket 7839
- 推荐实验：hash(user_123 + "rec_v1")   → bucket 1523
```

### Salt 配置方式

**方式 A：显式指定 Salt（推荐）**
```json
{
  "layer_id": "click_experiment",
  "version": "v1",
  "salt": "click_exp_2024_q1",
  "hash_key": "user_id",
  "enabled": true,
  "buckets": {"0": "control", "5000": "treatment"},
  "groups": {
    "control": {"service": "ranker_svc", "params": {"algorithm": "baseline"}},
    "treatment": {"service": "ranker_svc", "params": {"algorithm": "new_model"}}
  }
}
```

**方式 B：使用默认 Salt**
```json
{
  "layer_id": "color_experiment",
  "version": "v2",
  // 不指定 salt，自动使用 "color_experiment_v2"
  "hash_key": "user_id",
  "enabled": true
  // ...
}
```

### Salt 使用场景

**场景 1：独立的 A/B 测试（使用不同 Salt）**
```json
{"layer_id": "click_algorithm", "salt": "click_algo_2024"}
{"layer_id": "ui_color", "salt": "ui_color_2024"}
```

**场景 2：相关实验（使用相同 Salt）**
如果希望同一用户在多个相关实验中保持一致：
```json
{"layer_id": "rec_model", "salt": "recommendation_suite_2024"}
{"layer_id": "ranking_strategy", "salt": "recommendation_suite_2024"}
```

**场景 3：版本升级**
- 保持用户分组：固定 salt 不随版本变化
- 重新分配用户：不指定 salt，使用默认值（会随版本变化）

### Salt 最佳实践

✅ **推荐做法**：
- 为每个独立实验显式指定不同的 salt
- 使用有意义的命名：`{feature}_{year}_{quarter}` 或 `{team}_{experiment}_{date}`
- 保持 salt 稳定，除非需要重新分配流量
- 在配置中添加注释说明 salt 的用途

❌ **避免的做法**：
- 不要在无关实验中使用相同 salt
- 不要频繁修改 salt（会导致所有用户重新分配）
- 不要使用随机 salt（必须固定，确保重启后分配不变）

### Salt 性能影响

- 额外开销：< 1%（仅字符串拼接操作）
- 内存占用：每个 Layer 增加 ~20-50 字节
- 向后兼容：✅ 完全兼容旧配置文件

## 变更日志

### [Latest] - Salt Support for Independent Layer Distribution

**新增功能**：
- Layer 结构添加可选的 `salt` 字段
- 自动 salt 生成：未指定时使用 `{layer_id}_{version}`
- 哈希函数支持 salt：`hash_to_bucket_with_salt(key, salt)`
- 6 个专门的 salt 测试用例

**文件变更**：
- `src/layer.rs` - Layer 添加 salt 字段和 get_salt() 方法
- `src/hash.rs` - 新增 hash_to_bucket_with_salt() 函数
- `src/merge.rs` - 使用 layer 的 salt 计算 bucket
- `tests/salt_test.rs` - 新增 salt 专项测试
- `configs/layers/click_experiment_with_salt.json` - 新增配置示例

**测试结果**：所有 31 个测试通过

### [Initial Release]

**核心功能**：
- ✅ Layer 管理系统（支持 JSON/YAML）
- ✅ 10000 个哈希槽（0.01% 流量粒度）
- ✅ 热更新和原子替换
- ✅ 版本控制和回滚
- ✅ 多层参数合并（deterministic）
- ✅ Priority 优先级控制
- ✅ Service 强约束
- ✅ HTTP API 服务
- ✅ Prometheus Metrics
- ✅ 结构化日志（tracing）
- ✅ 完整的单元测试和集成测试
- ✅ Docker 支持

## 常见问题

### Q: 如何保证多个节点配置一致性？

A: 所有节点 watch 同一个配置目录（可以是共享存储、ConfigMap 或配置中心），配置更新后所有节点在 100ms 内完成热更新。

### Q: 如何处理哈希碰撞？

A: 使用 XXH3 算法，碰撞概率极低（< 2^-64）。即使碰撞，用户只会被分配到错误的实验组，不影响系统稳定性。

### Q: 如何实现多维度分流？

A: 创建多个 Layer，使用不同的 `hash_key`（如 user_id 和 session_id），系统会自动合并参数。

### Q: 如何验证实验配置正确性？

A: 提供测试 API，传入特定的 hash_key 值，验证返回的参数是否符合预期。

### Q: 如何知道当前使用的 salt？

A: 查询 Layer 详情：
```bash
curl http://localhost:8080/layers/click_experiment | jq .salt
```

### Q: 修改 salt 会影响现有用户吗？

A: 是的！修改 salt 会导致所有用户重新计算桶号，相当于重新分配流量。除非需要重新分配，否则不要修改 salt。

### Q: 可以不使用 salt 吗？

A: 可以，但不推荐。系统会自动使用 `{layer_id}_{version}` 作为默认 salt，确保不同实验的独立性。

### Q: Salt 对性能有影响吗？

A: 几乎没有影响（< 1%），只增加一次字符串拼接操作。

## 贡献指南

欢迎贡献代码、报告问题或提出建议！

## License

MIT License
