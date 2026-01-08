# 实验系统架构设计

## 整体架构

```
┌──────────────┐
│   Browser    │
└──────┬───────┘
       │ HTTPS (OIDC + JWT)
       ↓
┌──────────────────────────────────────┐
│         UI Service (Port 8081)       │
│  - Next.js / React                   │
│  - OIDC Provider (用户认证)          │
│  - RBAC (admin/developer/viewer)     │
│  - 审计日志                          │
│  - API Gateway                       │
└──────────────┬───────────────────────┘
               │ Internal gRPC/HTTP
               │ (Service Account Token)
               ↓
┌──────────────────────────────────────┐
│    Control Plane (Port 8082)         │
│  - Layer/Experiment CRUD             │
│  - 配置版本管理                      │
│  - AK/SK 认证（服务级别）            │
│  - gRPC Config Push Service          │
└──────────────┬───────────────────────┘
               │
               ├─→ PostgreSQL (配置存储)
               │
               └─→ gRPC Stream (Port 9091)
                   (AK/SK 认证)
                   ↓
            ┌──────────────────┐
            │   Data Plane     │
            │  - Rust Service  │
            │  - 实验评估      │
            │  - 配置热更新    │
            └──────────────────┘
```

## 服务职责划分

### 1. UI Service (ui_service/)

**职责：**
- 用户认证和授权（OIDC + RBAC）
- Web 前端托管
- API Gateway（转发到控制面）
- 用户操作审计日志
- WebSocket 实时通知（可选）

**技术栈：**
- Go + Gin（后端）
- Next.js / React（前端）
- PostgreSQL（用户、权限、审计日志）

**认证：**
```
用户 → OIDC 登录 → JWT Token → 访问 UI API
UI Service → Service Account Token → 调用控制面
```

**RBAC 角色：**
- `admin`: 完全控制（创建/更新/删除配置）
- `developer`: 创建/更新配置
- `viewer`: 只读访问

**API 示例：**
```
POST   /api/v1/auth/login
POST   /api/v1/auth/register
GET    /api/v1/layers
POST   /api/v1/layers          (requires: developer)
PUT    /api/v1/layers/:id      (requires: developer)
DELETE /api/v1/layers/:id      (requires: admin)
GET    /api/v1/experiments
POST   /api/v1/experiments     (requires: developer)
GET    /api/v1/audit-logs      (requires: admin)
```

### 2. Control Plane (control_plane/)

**职责：**
- 配置管理（Layer/Experiment CRUD）
- 配置版本管理和回滚
- gRPC 配置推送（长连接）
- 数据面实例管理
- 服务级别认证（AK/SK）

**技术栈：**
- Go + gRPC
- PostgreSQL（配置存储）

**认证：**
```
数据面 → AK/SK → 订阅配置推送
UI Service → Service Token → 调用 API
```

**API 示例（内部调用）：**
```go
// gRPC API（数据面调用）
rpc SubscribeConfig(SubscribeRequest) returns (stream ConfigChange);
rpc GetFullConfig(GetFullConfigRequest) returns (FullConfig);

// Internal HTTP API（UI Service 调用）
POST   /internal/layers
PUT    /internal/layers/:id
DELETE /internal/layers/:id
GET    /internal/data-planes
POST   /internal/versions/rollback
```

**AK/SK 管理：**
```sql
CREATE TABLE service_credentials (
    access_key VARCHAR(32) PRIMARY KEY,
    secret_key_hash VARCHAR(255) NOT NULL,
    service_name VARCHAR(255) NOT NULL,
    scopes JSONB NOT NULL,  -- ["read", "write"]
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW()
);
```

### 3. Data Plane (data_plane/)

**职责：**
- 实验评估和参数合并
- 订阅配置变更（gRPC 客户端）
- 高性能请求处理

**认证：**
- 启动时配置 AK/SK
- 连接控制面时使用 AK/SK 认证

## 认证流程

### 用户认证流程（OIDC）

```
1. 用户访问 UI → 302 重定向到 /auth/login
2. 输入邮箱/密码 → UI Service 验证
3. 生成 Authorization Code
4. 交换 Access Token + Refresh Token + ID Token (JWT)
5. 前端携带 JWT 访问 UI API
6. UI Service 验证 JWT + 检查 RBAC 权限
7. UI Service 使用 Service Token 调用控制面
```

### 服务认证流程（AK/SK）

```
1. 数据面启动，读取配置文件中的 AK/SK
2. 连接控制面 gRPC，携带 AK/SK
3. 控制面验证 AK/SK
   - 查询 service_credentials 表
   - 验证 secret_key_hash
   - 检查 enabled 状态
4. 建立长连接，推送配置变更
```

## 数据库设计

### UI Service 数据库

```sql
-- 用户表
CREATE TABLE users (
    id VARCHAR(36) PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL,  -- admin, developer, viewer
    created_at TIMESTAMP DEFAULT NOW()
);

-- OIDC 表（clients, tokens 等）
-- ...

-- 审计日志
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(36) REFERENCES users(id),
    action VARCHAR(50) NOT NULL,  -- create, update, delete
    resource_type VARCHAR(50) NOT NULL,  -- layer, experiment
    resource_id VARCHAR(255) NOT NULL,
    details JSONB,
    ip_address VARCHAR(50),
    created_at TIMESTAMP DEFAULT NOW()
);
```

### Control Plane 数据库

```sql
-- 配置表
CREATE TABLE layers (...);
CREATE TABLE experiments (...);
CREATE TABLE config_versions (...);

-- 数据面实例
CREATE TABLE data_plane_instances (
    id VARCHAR(255) PRIMARY KEY,
    access_key VARCHAR(32) UNIQUE NOT NULL,
    hostname VARCHAR(255),
    ip_address VARCHAR(50),
    status VARCHAR(50),  -- online, offline
    last_heartbeat TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW()
);

-- 服务凭证
CREATE TABLE service_credentials (
    access_key VARCHAR(32) PRIMARY KEY,
    secret_key_hash VARCHAR(255) NOT NULL,
    service_name VARCHAR(255) NOT NULL,
    scopes JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW()
);
```

## 配置推送流程

```
1. UI 用户修改 Layer 配置
   ↓
2. UI Service 验证 RBAC 权限
   ↓
3. UI Service 调用 Control Plane API（Service Token）
   ↓
4. Control Plane 保存配置 + 生成新版本
   ↓
5. Control Plane 广播配置变更（gRPC Stream）
   ↓
6. 所有订阅的数据面接收变更并应用
   ↓
7. Control Plane 记录数据面应用状态
   ↓
8. UI Service 通过 WebSocket 通知用户"配置已生效"
```

## 安全考虑

### 1. 用户认证（UI Service）
- HTTPS 强制
- OIDC 标准协议
- JWT Token 短期有效（1 小时）
- Refresh Token 长期有效（7 天）
- CSRF 保护
- Rate Limiting

### 2. 服务认证（Control Plane）
- AK/SK 生成：加密随机 32 字节
- SK 存储：bcrypt hash
- mTLS（可选）
- IP 白名单（可选）
- 请求签名（类似 AWS Signature V4）

### 3. 内部通信
- UI Service ↔ Control Plane：
  - 内网访问（不暴露到公网）
  - Service Account Token
  - 或直接 gRPC with mTLS

## 部署架构

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_DB: experiment_system
  
  control-plane:
    build: ./control_plane
    ports:
      - "8082:8082"  # Internal API
      - "9091:9091"  # gRPC Push
    environment:
      DATABASE_URL: postgres://postgres@postgres/experiment_system
      GRPC_PORT: 9091
  
  ui-service:
    build: ./ui_service
    ports:
      - "8081:8081"  # Public API
    environment:
      DATABASE_URL: postgres://postgres@postgres/experiment_system
      CONTROL_PLANE_URL: http://control-plane:8082
      OIDC_ISSUER: http://localhost:8081
  
  data-plane-1:
    build: ./data_plane
    ports:
      - "8080:8080"
    environment:
      CONTROL_PLANE_GRPC: control-plane:9091
      ACCESS_KEY: ${DATA_PLANE_AK}
      SECRET_KEY: ${DATA_PLANE_SK}
```

## 迁移路径

如果当前已有合并的实现，可以这样迁移：

1. **Phase 1**: 在现有 control_plane 中添加 AK/SK 认证
2. **Phase 2**: 将 OIDC 相关代码提取到新的 ui_service
3. **Phase 3**: UI 的 API 调用改为通过内部接口调用 control_plane
4. **Phase 4**: 拆分数据库（可选，也可以共用）

## 总结

| 维度 | UI Service | Control Plane |
|------|-----------|---------------|
| **暴露** | 公网 (HTTPS) | 内网 + gRPC 公开 |
| **认证** | OIDC + JWT | AK/SK |
| **权限** | RBAC (用户级别) | Scope (服务级别) |
| **职责** | 用户交互、审计 | 配置管理、推送 |
| **扩展** | 水平扩展（无状态） | 垂直扩展（gRPC 连接） |
| **语言** | Go + React/Next.js | Go |
| **端口** | 8081 | 8082 (API) + 9091 (gRPC) |

**核心优势：**
- ✅ 关注点分离：用户权限 vs 服务权限
- ✅ 独立扩展：UI 流量和配置推送流量隔离
- ✅ 安全性：内网 API 不暴露，gRPC 可以加 mTLS
- ✅ 灵活性：可以有多个 UI（Web、CLI、移动端）共享控制面
