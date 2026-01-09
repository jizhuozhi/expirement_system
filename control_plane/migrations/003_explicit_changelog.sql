-- 显式事务流水表方案（移除触发器）

-- 1. 删除旧的触发器（如果存在）
DROP TRIGGER IF EXISTS layer_change_notify ON layers;
DROP TRIGGER IF EXISTS experiment_change_notify ON experiments;
DROP FUNCTION IF EXISTS notify_layer_change();
DROP FUNCTION IF EXISTS notify_experiment_change();

-- 2. 确保流水表结构正确（极简版：只有4个字段）
CREATE TABLE IF NOT EXISTS config_change_log (
    id BIGSERIAL PRIMARY KEY,        -- 自增 ID（轮询依据）
    entity_type VARCHAR(20) NOT NULL, -- 'layer' 或 'experiment'
    entity_id VARCHAR(255) NOT NULL,  -- layer_id 或 eid
    operation VARCHAR(10) NOT NULL,   -- 'create', 'update', 'delete'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. 优化索引
CREATE INDEX IF NOT EXISTS idx_change_log_id ON config_change_log(id);
CREATE INDEX IF NOT EXISTS idx_change_log_created_at ON config_change_log(created_at);

-- 4. 清理旧数据（可选：保留最近7天的流水记录）
-- DELETE FROM config_change_log WHERE created_at < NOW() - INTERVAL '7 days';

-- 5. 添加注释
COMMENT ON TABLE config_change_log IS '配置变更流水表（显式事务方案）';
COMMENT ON COLUMN config_change_log.id IS '自增ID，轮询依据';
COMMENT ON COLUMN config_change_log.entity_type IS '实体类型：layer/experiment';
COMMENT ON COLUMN config_change_log.entity_id IS '实体ID：layer_id或eid';
COMMENT ON COLUMN config_change_log.operation IS '操作类型：create/update/delete';

-- 6. 示例：手动写入流水表的方式
/*
-- 在应用代码中，事务内同时执行：
BEGIN;
  -- 1. 更新实体
  UPDATE layers SET config = '...' WHERE layer_id = 'layer-123';
  
  -- 2. 写入流水表
  INSERT INTO config_change_log (entity_type, entity_id, operation)
  VALUES ('layer', 'layer-123', 'update');
COMMIT;
*/