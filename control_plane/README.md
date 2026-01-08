# 实验系统控制面

Go 实现的高性能控制面，提供实验配置管理、OIDC 认证、gRPC 配置推送。

## 核心特性

- ✅ **内置 OIDC Provider**：完整的 OAuth 2.0 / OIDC 认证
- ✅ **PostgreSQL 存储**：JSONB 字段高效存储复杂配置
- ✅ **gRPC 推送**：基于长连接的配置变更实时推送
- ✅ **RESTful API**：Layer 和 Experiment 管理
- ✅ **WebSocket 通知**：浏览器端实时接收变更
- ✅ **多租户支持**：基于用户角色的权限控制

## 快速开始

### 1. 安装依赖

```bash
cd control_plane

# 安装 Go 依赖
go mod download

# 安装 protoc 和插件
brew install protobuf
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
```

### 2. 生成 Protobuf 代码

```bash
make proto
```

### 3. 启动 PostgreSQL

```bash
docker run -d \
  --name experiment-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=experiment_control \
  -p 5432:5432 \
  postgres:16
```

### 4. 运行数据库迁移

```bash
make migrate
```

### 5. 启动控制面

```bash
# 开发模式
make dev

# 或生产模式
make build
make run
```

服务监听：
- HTTP API: `http://localhost:8081`
- gRPC: `localhost:9091`

## API 示例

### 认证

```bash
# 注册用户
curl -X POST http://localhost:8081/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "password123",
    "name": "Test User"
  }'

# 登录
curl -X POST http://localhost:8081/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "password123"
  }'
```

### Layer 管理

```bash
# 创建 Layer
curl -X POST http://localhost:8081/api/v1/layers \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "layer_id": "test_layer",
    "priority": 100,
    "hash_key": "user_id",
    "salt": "test_salt",
    "enabled": true,
    "ranges": [
      {"start": 0, "end": 5000, "vid": 1001},
      {"start": 5000, "end": 10000, "vid": 1002}
    ],
    "services": ["api_service"]
  }'

# 列出所有 Layers
curl http://localhost:8081/api/v1/layers \
  -H "Authorization: Bearer $TOKEN"

# 更新 Layer
curl -X PUT http://localhost:8081/api/v1/layers/test_layer \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "enabled": false
  }'

# 删除 Layer
curl -X DELETE http://localhost:8081/api/v1/layers/test_layer \
  -H "Authorization: Bearer $TOKEN"
```

### Experiment 管理

```bash
# 创建 Experiment
curl -X POST http://localhost:8081/api/v1/experiments \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "service": "api_service",
    "name": "New Algorithm Test",
    "rule": {
      "type": "field",
      "field": "country",
      "op": "eq",
      "values": ["US"]
    },
    "variants": [
      {
        "vid": 1001,
        "params": {"algorithm": "baseline"}
      },
      {
        "vid": 1002,
        "params": {"algorithm": "new_model"}
      }
    ]
  }'
```

## gRPC 配置推送

数据面通过 gRPC 订阅配置变更：

```go
// 数据面客户端示例
conn, _ := grpc.Dial("localhost:9091", grpc.WithInsecure())
client := pb.NewConfigPushServiceClient(conn)

stream, _ := client.SubscribeConfig(context.Background(), &pb.SubscribeRequest{
    DataPlaneId: "data-plane-1",
    Version:     "v1.0.0",
    Services:    []string{"api_service"},
})

for {
    change, err := stream.Recv()
    if err != nil {
        break
    }
    
    // 处理配置变更
    switch change.Type {
    case pb.ConfigChange_FULL_RELOAD:
        // 全量重载
    case pb.ConfigChange_LAYER_UPDATE:
        // 更新 Layer
    case pb.ConfigChange_LAYER_DELETE:
        // 删除 Layer
    }
}
```

## 架构设计

### 数据模型

```
users               用户表
  ├─ layers         实验层表
  ├─ experiments    实验表
  └─ config_versions 配置版本表

data_plane_instances  数据面实例表（订阅管理）

oidc_clients          OIDC 客户端
oidc_authorization_codes  授权码
oidc_access_tokens    访问令牌
oidc_refresh_tokens   刷新令牌
```

### 配置推送流程

```
控制面更新配置
    ↓
生成新版本号
    ↓
保存到数据库
    ↓
广播到所有订阅者（gRPC Stream）
    ↓
数据面接收并应用
```

### OIDC 认证流程

```
1. 用户访问 /oauth/authorize
2. 登录后生成 authorization_code
3. 客户端用 code 换取 access_token
4. 使用 access_token 访问 API
5. Token 过期后用 refresh_token 刷新
```

## 配置文件

`config.yaml`:

```yaml
server:
  host: 0.0.0.0
  port: 8081

database:
  host: localhost
  port: 5432
  user: postgres
  password: postgres
  database: experiment_control

oidc:
  issuer: http://localhost:8081
  jwt_secret: your-secret-key-change-me
  access_ttl: 3600      # 1 hour
  refresh_ttl: 604800   # 7 days

grpc:
  host: 0.0.0.0
  port: 9091

log:
  level: info
```

## 开发

### 项目结构

```
control_plane/
├── cmd/server/          # 主程序
├── internal/
│   ├── config/          # 配置加载
│   ├── grpc_server/     # gRPC 推送服务
│   ├── handler/         # HTTP handlers
│   ├── service/         # 业务逻辑
│   ├── repository/      # 数据库访问
│   ├── auth/            # 认证中间件
│   ├── models/          # 数据模型
│   └── middleware/      # HTTP 中间件
├── pkg/
│   ├── oidc/            # OIDC Provider
│   └── utils/           # 工具函数
├── proto/               # Protobuf 定义
├── migrations/          # 数据库迁移
└── config.yaml          # 配置文件
```

### 命令

```bash
make build      # 构建
make run        # 运行
make dev        # 开发模式
make proto      # 生成 protobuf
make migrate    # 数据库迁移
make test       # 测试
make clean      # 清理
```

## 与数据面集成

数据面需要：

1. 实现 gRPC 客户端，订阅配置变更
2. 处理不同类型的变更消息
3. 定期发送心跳保持连接
4. 断线重连时拉取全量配置

示例集成代码见数据面 README。

## 安全建议

1. **生产环境**：修改 `jwt_secret` 为强随机字符串
2. **HTTPS**：使用 TLS 保护 HTTP 和 gRPC 通信
3. **数据库**：启用 SSL 连接，使用强密码
4. **Token TTL**：根据安全需求调整令牌过期时间
5. **RBAC**：实现细粒度的角色权限控制

## TODO

- [ ] 实现完整的 CRUD handlers
- [ ] WebSocket 实时通知
- [ ] 审计日志
- [ ] Metrics 监控
- [ ] Rate Limiting
- [ ] 配置回滚功能
- [ ] 多环境管理
- [ ] 前端 UI

## License

MIT
