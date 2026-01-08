# 流水表轮询配置推送方案（极简版）

## 核心思想

**数据库触发器记录变更 ID，控制面轮询流水表，反查实体**

## 架构图

```
┌────────────────────────────────────────────────────────┐
│                    PostgreSQL                           │
│                                                         │
│  ┌─────────┐  Trigger  ┌─────────────────────┐        │
│  │ layers  │──────────→│ config_change_log   │        │
│  │experiments│          │  - id (自增)         │        │
│  └─────────┘           │  - entity_type       │        │
│                         │  - entity_id         │        │
│                         │  - operation         │        │
│                         └─────────────────────┘        │
└────────────────────────────────────────────────────────┘
             ↑                    ↑                ↑
             │ Poll              │                │
    ┌────────┴────────┐  ┌───────┴───────┐  ┌────┴─────┐
    │  Control-1      │  │  Control-2    │  │Control-3 │
    │  lastID: 1000   │  │  lastID: 1000 │  │lastID:999│
    └────────┬────────┘  └───────┬───────┘  └────┬─────┘
             │                    │               │
             ↓                    ↓               ↓
       Data Planes          Data Planes     Data Planes
```

## 工作流程

### 1. 配置变更（Control-1 收到请求）

```
HTTP API: POST /api/layers
    ↓
1. INSERT INTO layers (layer_id='layer-123', ...)
    ↓
2. 触发器自动写入流水表:
   INSERT INTO config_change_log (
       id: 1001,              -- 自增
       entity_type: 'layer',
       entity_id: 'layer-123',
       operation: 'create'
   )
    ↓
3. Control-1 更新本地内存 + 推送本节点数据面
```

### 2. 其他节点感知变更（Control-2/3）

```
轮询（每 1 秒）:
SELECT id, entity_type, entity_id, operation
FROM config_change_log
WHERE id > 1000  -- lastID
ORDER BY id ASC
LIMIT 1000
    ↓
返回: [{id:1001, entity_type:'layer', entity_id:'layer-123', operation:'create'}]
    ↓
反查实体:
SELECT * FROM layers WHERE layer_id = 'layer-123'
    ↓
更新本地内存 + 推送本节点数据面 + lastID=1001
```

## 数据库表结构（极简）

```sql
CREATE TABLE config_change_log (
    id BIGSERIAL PRIMARY KEY,        -- 自增 ID（轮询依据）
    entity_type VARCHAR(20) NOT NULL, -- 'layer' 或 'experiment'
    entity_id VARCHAR(255) NOT NULL,  -- layer_id 或 eid
    operation VARCHAR(10) NOT NULL,   -- 'create', 'update', 'delete'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_change_log_id ON config_change_log(id);
```

**只有 4 个字段！极简到极致。**

## 触发器（自动记录变更）

```sql
-- Layer 变更触发器
CREATE FUNCTION log_layer_change() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO config_change_log (entity_type, entity_id, operation)
    VALUES ('layer', 
            CASE TG_OP 
                WHEN 'DELETE' THEN OLD.layer_id 
                ELSE NEW.layer_id 
            END,
            LOWER(TG_OP));
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER log_layer_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH ROW EXECUTE FUNCTION log_layer_change();
```

## 为什么是最优方案？

| 方案 | 存储 | 组件 | 复杂度 |
|------|------|------|--------|
| **流水表** | PostgreSQL | 0 | ⭐ |
| LISTEN/NOTIFY | PostgreSQL | 0 | ⭐⭐ (长连接) |
| Consul/etcd | PG + Consul | 1 | ⭐⭐⭐ (双写) |
| Gossip | PostgreSQL | 0 | ⭐⭐⭐⭐ (冲突) |
| CDC | PG + Kafka | 2 | ⭐⭐⭐⭐⭐ |

**流水表 = 零额外组件 + 最简单**

## 关键优势

✅ **极简** - 只需 4 个字段  
✅ **零组件** - 无 Kafka/Consul/etcd  
✅ **强一致** - 数据库是唯一真相来源  
✅ **反查实体** - 保证数据最新（避免传输过时数据）  
✅ **自动恢复** - 节点离线后追上 ID  
✅ **事务保证** - 触发器在同一事务内  

## 实现要点

### 1. 轮询器

```go
// 每 1 秒轮询一次
poller := NewChangeLogPoller(db, 1*time.Second, handler, logger)

// 查询新记录
SELECT id, entity_type, entity_id, operation, created_at
FROM config_change_log
WHERE id > lastID
ORDER BY id ASC
LIMIT 1000
```

### 2. 处理变更

```go
func HandleChangeLog(entry *ChangeLogEntry) error {
    switch entry.EntityType {
    case "layer":
        if entry.Operation == "delete" {
            // 删除：直接删内存
            delete(layers, entry.EntityID)
        } else {
            // 创建/更新：反查数据库获取完整实体
            layer := repo.GetLayer(entry.EntityID)
            layers[entry.EntityID] = layer
        }
        
        // 推送给本节点的数据面
        pushToLocalDataPlanes(layer)
    }
}
```

### 3. 为什么反查实体？

```
❌ 流水表存完整数据:
- 流水表巨大（重复存储）
- 传输过时数据（更新后旧版本还在流水表）

✅ 只存 ID，反查实体:
- 流水表极小（只存 4 个字段）
- 保证最新（从主表查询当前数据）
- 删除操作无需反查（直接删内存即可）
```

## 适用场景

✅ **配置变更频率低** (< 10 次/秒)  
✅ **可接受秒级延迟** (1-5 秒)  
✅ **需要强一致性**  
✅ **追求极简运维**  

## 总结

这是**最简单、最可靠**的多实例配置同步方案：

- **4 个字段** 的流水表
- **反查实体** 保证数据最新
- **零外部组件**
- **完美适合实验系统**

## 架构图

```
┌────────────────────────────────────────────────────────────┐
│                      PostgreSQL                             │
│                                                             │
│  ┌─────────┐    Trigger    ┌──────────────────────┐       │
│  │ layers  │──────────────→│ config_change_log    │       │
│  │experiments│              │  - id (自增)          │       │
│  └─────────┘               │  - entity_type        │       │
│                             │  - operation          │       │
│                             │  - entity_id          │       │
│                             └──────────────────────┘       │
└────────────────────────────────────────────────────────────┘
                  ↑                      ↑                ↑
                  │ Poll (lastID)       │                │
         ┌────────┴────────┐  ┌─────────┴────────┐  ┌───┴──────┐
         │  Control-1      │  │  Control-2       │  │Control-3 │
         │  lastID: 1000   │  │  lastID: 1000    │  │lastID:999│
         └────────┬────────┘  └─────────┬────────┘  └───┬──────┘
                  │                      │               │
                  ↓                      ↓               ↓
            Data Planes             Data Planes    Data Planes
```

## 工作流程

### 1. 配置变更流程（以 Layer 创建为例）

```
用户 → HTTP API (Control-1: POST /api/layers)
         ↓
    1. INSERT INTO layers (...)
         ↓
    2. 数据库触发器自动执行
       INSERT INTO config_change_log (
           id: 1001,                    -- 自增
           entity_type: 'layer',
           operation: 'create',
           entity_id: 'layer-123',
           version: 1234567890
       )
         ↓
    3. Control-1 本地内存更新
    4. Control-1 推送给本节点数据面
```

### 2. 其他节点感知变更

```
Control-2/3 轮询（每 1 秒）
    ↓
SELECT * FROM config_change_log
WHERE id > lastID  -- Control-2: id > 1000
ORDER BY id ASC
LIMIT 1000
    ↓
返回：[{id: 1001, entity_type: 'layer', operation: 'create', entity_id: 'layer-123'}]
    ↓
1. 从数据库加载完整数据: SELECT * FROM layers WHERE layer_id = 'layer-123'
2. 更新本地内存
3. 推送给本节点的数据面
4. 更新 lastID = 1001
```

## 关键设计

### 1. 为什么不用 Gossip/Consul/etcd？

| 方案 | 存储数量 | 复杂度 | 说明 |
|------|---------|--------|------|
| **流水表** | 1 (PostgreSQL) | ⭐ 极简 | ✅ 推荐 |
| Consul/etcd | 2 (PG + Consul) | ⭐⭐ 中等 | 双写异构存储 |
| Gossip | 1 (PG + 内存) | ⭐⭐⭐ 复杂 | 需要冲突解决、反熵 |
| CDC | 2 (PG + Kafka) | ⭐⭐⭐⭐ 很复杂 | 依赖 Debezium/Kafka |

**结论：流水表是最简单且唯一不需要额外组件的方案**

### 2. 数据库触发器自动记录

```sql
-- Layer 变更触发器
CREATE TRIGGER log_layer_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH ROW EXECUTE FUNCTION log_layer_change();

-- 触发器函数
CREATE FUNCTION log_layer_change() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO config_change_log (entity_type, operation, entity_id, ...)
    VALUES ('layer', TG_OP, NEW.layer_id, ...);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

**优势：**
- CRUD Handler 无需关心流水表
- 事务保证：配置写入成功 → 流水记录一定存在
- 不会遗漏任何变更

### 3. 轮询器（ChangeLogPoller）

```go
type ChangeLogPoller struct {
    lastID   int64          // 上次轮询的最大 ID
    interval time.Duration  // 轮询间隔（推荐 1-5 秒）
}

func (p *ChangeLogPoller) poll() {
    // 1. 查询新记录
    rows := db.Query(`
        SELECT * FROM config_change_log
        WHERE id > $1
        ORDER BY id ASC
        LIMIT 1000
    `, p.lastID)
    
    // 2. 处理每条记录
    for row := range rows {
        handler(row)  // 由 ConfigState 实现
        p.lastID = max(p.lastID, row.ID)
    }
}
```

**特点：**
- 轮询间隔可配置（1-5 秒）
- ID 保证严格递增，不会丢失
- 批量处理（LIMIT 1000）

### 4. 处理变更（ConfigState）

```go
func (s *ConfigState) HandleChangeLog(entry *ChangeLogEntry) error {
    switch entry.EntityType {
    case "layer":
        if entry.Operation == "delete" {
            // 删除：直接删内存
            delete(s.layers, entry.EntityID)
        } else {
            // 创建/更新：从数据库重新加载
            layer := repo.GetLayer(entry.EntityID)
            s.layers[entry.EntityID] = layer
        }
        
        // 推送给本节点的数据面
        s.pushToLocalDataPlanes(layer)
    }
}
```

**要点：**
- 创建/更新操作：从数据库加载完整数据（保证最新）
- 删除操作：只需删内存
- 只推送给本节点的数据面订阅者

## 对比其他方案

### vs. LISTEN/NOTIFY

| 维度 | 流水表轮询 | LISTEN/NOTIFY |
|------|-----------|---------------|
| 可靠性 | ✅ 持久化，重启不丢 | ❌ 连接断开丢失 |
| 延迟 | 1-5 秒 | < 100ms |
| 复杂度 | 简单 | 需要维护长连接 |
| 网络分区 | ✅ 分区恢复后自动追上 | ❌ 可能丢失 |

### vs. Gossip

| 维度 | 流水表轮询 | Gossip |
|------|-----------|--------|
| 一致性 | ✅ 强一致（DB 为准） | 最终一致 |
| 冲突处理 | 无需处理 | 需要 Vector Clock/CRDT |
| 延迟 | 1-5 秒 | 10-100ms |
| 运维 | ✅ 零配置 | 需要配置 peers |

### vs. CDC (Debezium + Kafka)

| 维度 | 流水表轮询 | CDC |
|------|-----------|-----|
| 组件 | PostgreSQL | PostgreSQL + Kafka + Debezium |
| 复杂度 | ⭐ | ⭐⭐⭐⭐ |
| 延迟 | 1-5 秒 | < 1 秒 |
| 成本 | ✅ 免费 | 需要 Kafka 集群 |

## 实现细节

### 数据库表结构

```sql
CREATE TABLE config_change_log (
    id BIGSERIAL PRIMARY KEY,           -- 自增 ID（保证顺序）
    entity_type VARCHAR(50) NOT NULL,   -- 'layer', 'experiment'
    operation VARCHAR(20) NOT NULL,     -- 'create', 'update', 'delete'
    entity_id VARCHAR(255) NOT NULL,    -- layer_id 或 eid
    service VARCHAR(100),                -- 所属服务（可选）
    version BIGINT NOT NULL,             -- 版本号
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 核心索引
CREATE INDEX idx_change_log_id ON config_change_log(id);
```

### 轮询参数

```go
// 推荐配置
poller := NewChangeLogPoller(
    db,
    interval: 1 * time.Second,  // 1 秒轮询一次
    handler:  configState.HandleChangeLog,
    logger:   logger,
)
```

**为什么 1 秒足够？**
- 配置变更频率通常很低（< 1 次/秒）
- 1 秒延迟对配置推送完全可接受
- 降低数据库压力

### 流水表清理

```sql
-- 定期清理 7 天前的数据
DELETE FROM config_change_log
WHERE created_at < NOW() - INTERVAL '7 days';
```

**为什么保留 7 天？**
- 覆盖节点离线恢复场景
- 超过 7 天的节点应该从数据库全量加载

## 优势总结

✅ **极简架构** - 只需 PostgreSQL，无额外组件  
✅ **零配置** - 无需配置 peers、Kafka 等  
✅ **强一致性** - 数据库是唯一真相来源  
✅ **自动恢复** - 节点离线/重启后自动追上  
✅ **事务保证** - 配置写入成功 → 流水记录一定存在  
✅ **可观测** - 流水表可直接查询、审计  
✅ **低成本** - 不增加运维成本  

## 适用场景

✅ **配置变更频率低**（< 10 次/秒）  
✅ **可接受秒级延迟**（1-5 秒）  
✅ **需要强一致性**  
✅ **追求运维简单**  

## 不适用场景

❌ **毫秒级延迟需求** → 用 LISTEN/NOTIFY 或 Gossip  
❌ **超高频变更**（> 100 次/秒） → 用 Kafka  
❌ **需要历史回溯** → 保留完整 CDC 流  

## 总结

流水表轮询是**最简单、最可靠、最易运维**的配置同步方案，特别适合实验系统这种配置变更频率低、一致性要求高的场景。

相比 Gossip、Consul、CDC 等方案，它的核心优势是：**零外部依赖 + 强一致性**。
