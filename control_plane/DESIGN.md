# 控制面设计文档

## 配置推送机制

### 架构流程

```
┌─────────────┐
│  UI / API   │
└──────┬──────┘
       │ HTTP CRUD
       ↓
┌─────────────────────────────────────────┐
│         Control Plane Service           │
│  ┌────────────┐      ┌───────────────┐ │
│  │   Handler  │──→   │  Repository   │ │
│  └────────────┘      └───────┬───────┘ │
└──────────────────────────────┼─────────┘
                               ↓
                    ┌──────────────────┐
                    │   PostgreSQL     │
                    │  ┌────────────┐  │
                    │  │  Trigger   │  │
                    │  └──────┬─────┘  │
                    │         ↓        │
                    │  pg_notify()     │
                    └──────────┬───────┘
                               ↓ NOTIFY 'config_changes'
                    ┌──────────────────┐
                    │   PgNotifier     │
                    │  (LISTEN loop)   │
                    └──────────┬───────┘
                               ↓
                    ┌──────────────────┐
                    │   PushServer     │
                    │  (broadcast)     │
                    └──────────┬───────┘
                               ↓ gRPC Stream
                    ┌──────────────────┐
                    │   Data Planes    │
                    │  (subscribers)   │
                    └──────────────────┘
```

### 详细流程

#### 1. CRUD 操作

```go
// Handler
func (h *LayerHandler) UpdateLayer(c *gin.Context) {
    var req UpdateLayerRequest
    c.BindJSON(&req)
    
    // 直接更新数据库，不需要手动推送
    err := h.repo.UpdateLayer(ctx, layerID, &req)
    if err != nil {
        c.JSON(500, gin.H{"error": err.Error()})
        return
    }
    
    c.JSON(200, gin.H{"message": "updated"})
    // 数据库触发器会自动 NOTIFY
}
```

#### 2. 数据库触发器

```sql
-- 在 INSERT/UPDATE/DELETE 时自动触发
CREATE TRIGGER layer_change_notify
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH ROW EXECUTE FUNCTION notify_layer_change();

-- 触发器函数
CREATE OR REPLACE FUNCTION notify_layer_change()
RETURNS TRIGGER AS $$
DECLARE
    payload JSON;
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        payload = json_build_object(
            'operation', 'UPDATE',
            'table', 'layers',
            'layer_id', NEW.layer_id,
            'timestamp', extract(epoch from now())::bigint
        );
        -- 发送通知到 'config_changes' 频道
        PERFORM pg_notify('config_changes', payload::text);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

#### 3. PgNotifier 监听

```go
// notifier/pg_notifier.go
func (n *PgNotifier) Start(ctx context.Context) error {
    conn, _ := n.pool.Acquire(ctx)
    defer conn.Release()
    
    // LISTEN 指定频道
    conn.Exec(ctx, "LISTEN config_changes")
    
    for {
        // 阻塞等待通知
        notification, err := conn.Conn().WaitForNotification(ctx)
        if err != nil {
            continue
        }
        
        // 解析 JSON payload
        var change ChangeNotification
        json.Unmarshal([]byte(notification.Payload), &change)
        
        // 调用所有注册的 handlers
        for _, handler := range n.handlers {
            handler(&change)
        }
    }
}
```

#### 4. PushServer 广播

```go
// grpc_server/push_server.go
func (s *PushServer) HandleDBChange(change *notifier.ChangeNotification) error {
    // 根据变更类型构造 ConfigChange
    configChange := &pb.ConfigChange{
        Type:      pb.ConfigChange_LAYER_UPDATE,
        Version:   fmt.Sprintf("v%d", time.Now().Unix()),
        Timestamp: change.Timestamp,
        Layers:    []*pb.Layer{ /* 从数据库加载 */ },
    }
    
    // 广播到所有订阅者
    s.BroadcastChange(configChange)
    return nil
}

func (s *PushServer) BroadcastChange(change *pb.ConfigChange) {
    // 遍历所有订阅者
    s.subscribers.Range(func(key, value interface{}) bool {
        sub := value.(*Subscriber)
        
        // 发送到订阅者的 channel
        select {
        case sub.Updates <- change:
        default:
            // channel 满了，记录日志
        }
        return true
    })
}
```

#### 5. gRPC Stream 推送

```go
// 订阅者的 goroutine
func (s *PushServer) SubscribeConfig(req *pb.SubscribeRequest, stream pb.ConfigPushService_SubscribeConfigServer) error {
    sub := &Subscriber{
        ID:      req.DataPlaneId,
        Updates: make(chan *pb.ConfigChange, 10),
    }
    
    s.subscribers.Store(req.DataPlaneId, sub)
    defer s.subscribers.Delete(req.DataPlaneId)
    
    // 监听 Updates channel
    for {
        select {
        case change := <-sub.Updates:
            // 通过 gRPC stream 发送
            stream.Send(change)
        case <-stream.Context().Done():
            return nil
        }
    }
}
```

## 方案优势

### 1. 解耦
- CRUD Handler **不需要关心**推送逻辑
- 只需要操作数据库，触发器自动处理

### 2. 可靠性
- 数据库事务保证：写入成功 → NOTIFY 一定触发
- NOTIFY 在事务提交后发送（AFTER 触发器）
- 避免推送"未提交的变更"

### 3. 零延迟
- PostgreSQL NOTIFY 是**即时**的（毫秒级）
- 不需要轮询

### 4. 简单
- 零外部依赖（无需 Kafka/Redis）
- PostgreSQL 原生特性

## 注意事项

### 1. NOTIFY 限制

**Payload 大小限制：8000 字节**
```sql
-- 只发送变更的标识，不发送完整数据
payload = json_build_object(
    'operation', 'UPDATE',
    'layer_id', NEW.layer_id  -- ✅ 只发送 ID
    -- 'data', row_to_json(NEW)  -- ❌ 不要发送完整行
);
```

**解决方案：**
- NOTIFY 只发送"什么变了"（ID、操作类型）
- PushServer 收到通知后，再从数据库加载完整数据

### 2. NOTIFY 不持久化

如果 PgNotifier 断连，会丢失断连期间的通知。

**解决方案：**
- PgNotifier 重连后，查询 `config_versions` 表
- 对比数据面的 `current_version`，决定是否全量推送

```go
func (n *PgNotifier) Start(ctx context.Context) error {
    for {
        // 尝试连接
        if err := n.listen(ctx); err != nil {
            logger.Error("listen failed, reconnecting...", zap.Error(err))
            time.Sleep(5 * time.Second)
            
            // 重连后检查是否有遗漏的变更
            n.checkMissedChanges(ctx)
        }
    }
}
```

### 3. 多实例控制面

如果部署多个控制面实例，每个都会收到 NOTIFY。

**解决方案：**
- 多个实例可以同时处理（幂等性）
- 或使用分布式锁（Redis/etcd）确保只有一个实例处理

```go
func (s *PushServer) HandleDBChange(change *notifier.ChangeNotification) error {
    // 获取分布式锁
    lock := s.redis.Lock(fmt.Sprintf("config_change:%s:%d", change.Table, change.Timestamp))
    if !lock.TryLock() {
        return nil // 其他实例已处理
    }
    defer lock.Unlock()
    
    // 处理变更
    // ...
}
```

## 替代方案对比

### CDC (Debezium)

**优点：**
- 持久化（Kafka）
- 支持多种数据库

**缺点：**
- 复杂（需要 Kafka + Debezium）
- 延迟略高（毫秒到秒级）
- 运维成本

**适用场景：**
- 需要历史回溯
- 多个下游消费者
- 已有 Kafka 基础设施

### 应用层推送

```go
func (h *LayerHandler) UpdateLayer(c *gin.Context) {
    // 更新数据库
    h.repo.UpdateLayer(ctx, layer)
    
    // 手动推送
    h.pushServer.BroadcastLayerUpdate(layer)  // ⚠️ 如果失败怎么办？
}
```

**问题：**
- 不可靠：数据库成功，推送失败 → 不一致
- 耦合：Handler 需要感知 PushServer

**何时使用：**
- 推送非关键（可丢失）
- 极简场景

## 最佳实践

### 1. 版本管理

每次变更生成新版本号：

```sql
-- 在触发器中自动创建版本
INSERT INTO config_versions (version, change_log, created_by)
VALUES ('v' || extract(epoch from now())::bigint, 'Layer updated: ' || NEW.layer_id, current_user);
```

### 2. 批量变更

如果一次更新多个 Layer，避免多次 NOTIFY：

```sql
-- 使用 STATEMENT 级别触发器
CREATE TRIGGER layer_change_notify
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH STATEMENT  -- 注意：STATEMENT 而不是 ROW
    EXECUTE FUNCTION notify_layer_batch_change();
```

### 3. 过滤订阅

数据面只订阅关心的 service：

```go
// 数据面订阅时指定 services
stream, _ := client.SubscribeConfig(ctx, &pb.SubscribeRequest{
    Services: []string{"api_service"},
})

// 控制面过滤推送
func (s *PushServer) BroadcastChange(change *pb.ConfigChange) {
    s.subscribers.Range(func(key, value interface{}) bool {
        sub := value.(*Subscriber)
        
        // 检查 service 是否匹配
        if !sub.InterestedIn(change) {
            return true // skip
        }
        
        sub.Updates <- change
        return true
    })
}
```

## 监控指标

```go
// Prometheus metrics
var (
    notifyReceived = prometheus.NewCounterVec(
        prometheus.CounterOpts{
            Name: "config_notify_received_total",
        },
        []string{"table", "operation"},
    )
    
    broadcastSent = prometheus.NewCounterVec(
        prometheus.CounterOpts{
            Name: "config_broadcast_sent_total",
        },
        []string{"type"},
    )
    
    subscriberCount = prometheus.NewGauge(
        prometheus.GaugeOpts{
            Name: "grpc_subscribers_total",
        },
    )
)
```

## 总结

**推荐使用 PostgreSQL LISTEN/NOTIFY**，因为：
- ✅ 简单：零外部依赖
- ✅ 可靠：事务保证
- ✅ 实时：毫秒级延迟
- ✅ 解耦：CRUD 不感知推送

对于大多数场景，这是**最佳方案**。只有在需要历史回溯、多数据源同步时，才考虑 CDC。
