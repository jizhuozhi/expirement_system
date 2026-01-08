-- 用户表
CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(36) PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'viewer',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);

-- Layer 表
CREATE TABLE IF NOT EXISTS layers (
    layer_id VARCHAR(255) PRIMARY KEY,
    version VARCHAR(50) NOT NULL,
    priority INTEGER NOT NULL,
    hash_key VARCHAR(255) NOT NULL,
    salt VARCHAR(255) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    ranges JSONB NOT NULL,
    services JSONB NOT NULL DEFAULT '[]',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_by VARCHAR(36) REFERENCES users(id),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_layers_priority ON layers(priority);
CREATE INDEX idx_layers_enabled ON layers(enabled);
CREATE INDEX idx_layers_services ON layers USING GIN(services);

-- Experiment 表
CREATE TABLE IF NOT EXISTS experiments (
    eid SERIAL PRIMARY KEY,
    service VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    rule JSONB NOT NULL,
    variants JSONB NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    status VARCHAR(50) NOT NULL DEFAULT 'draft',
    created_by VARCHAR(36) REFERENCES users(id),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_experiments_service ON experiments(service);
CREATE INDEX idx_experiments_status ON experiments(status);

-- 配置版本表
CREATE TABLE IF NOT EXISTS config_versions (
    version VARCHAR(50) PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    change_log TEXT NOT NULL,
    created_by VARCHAR(36) REFERENCES users(id),
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_config_versions_timestamp ON config_versions(timestamp);

-- 数据面实例表
CREATE TABLE IF NOT EXISTS data_plane_instances (
    id VARCHAR(255) PRIMARY KEY,
    hostname VARCHAR(255) NOT NULL,
    ip_address VARCHAR(50) NOT NULL,
    version VARCHAR(50) NOT NULL,
    current_version VARCHAR(50) REFERENCES config_versions(version),
    last_heartbeat TIMESTAMP NOT NULL DEFAULT NOW(),
    status VARCHAR(50) NOT NULL DEFAULT 'online',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_data_plane_status ON data_plane_instances(status);
CREATE INDEX idx_data_plane_heartbeat ON data_plane_instances(last_heartbeat);

-- OIDC Client 表
CREATE TABLE IF NOT EXISTS oidc_clients (
    id VARCHAR(255) PRIMARY KEY,
    secret VARCHAR(255) NOT NULL,
    redirect_uris JSONB NOT NULL,
    grant_types JSONB NOT NULL,
    response_types JSONB NOT NULL,
    scopes JSONB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- OIDC 授权码表
CREATE TABLE IF NOT EXISTS oidc_authorization_codes (
    code VARCHAR(255) PRIMARY KEY,
    client_id VARCHAR(255) NOT NULL REFERENCES oidc_clients(id),
    user_id VARCHAR(36) NOT NULL REFERENCES users(id),
    redirect_uri TEXT NOT NULL,
    scopes JSONB NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_auth_codes_expires ON oidc_authorization_codes(expires_at);

-- OIDC Access Token 表
CREATE TABLE IF NOT EXISTS oidc_access_tokens (
    token VARCHAR(255) PRIMARY KEY,
    client_id VARCHAR(255) NOT NULL REFERENCES oidc_clients(id),
    user_id VARCHAR(36) NOT NULL REFERENCES users(id),
    scopes JSONB NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_access_tokens_expires ON oidc_access_tokens(expires_at);
CREATE INDEX idx_access_tokens_user ON oidc_access_tokens(user_id);

-- OIDC Refresh Token 表
CREATE TABLE IF NOT EXISTS oidc_refresh_tokens (
    token VARCHAR(255) PRIMARY KEY,
    client_id VARCHAR(255) NOT NULL REFERENCES oidc_clients(id),
    user_id VARCHAR(36) NOT NULL REFERENCES users(id),
    scopes JSONB NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_refresh_tokens_expires ON oidc_refresh_tokens(expires_at);

-- 更新时间触发器
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_layers_updated_at BEFORE UPDATE ON layers
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_experiments_updated_at BEFORE UPDATE ON experiments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_data_plane_updated_at BEFORE UPDATE ON data_plane_instances
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================
-- 配置变更流水表（极简版：只记录变更 ID）
-- ============================================

CREATE TABLE config_change_log (
    id BIGSERIAL PRIMARY KEY,           -- 自增 ID（控制面轮询这个字段）
    entity_type VARCHAR(20) NOT NULL,   -- 'layer' 或 'experiment'
    entity_id VARCHAR(255) NOT NULL,    -- layer_id 或 eid
    operation VARCHAR(10) NOT NULL,     -- 'create', 'update', 'delete'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_change_log_id ON config_change_log(id);
CREATE INDEX idx_change_log_created_at ON config_change_log(created_at);

-- Layer 变更触发器
CREATE OR REPLACE FUNCTION log_layer_change()
RETURNS TRIGGER AS $$
BEGIN
    IF (TG_OP = 'DELETE') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('layer', OLD.layer_id, 'delete');
        RETURN OLD;
    ELSIF (TG_OP = 'UPDATE') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('layer', NEW.layer_id, 'update');
        RETURN NEW;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('layer', NEW.layer_id, 'create');
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER log_layer_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH ROW EXECUTE FUNCTION log_layer_change();

-- Experiment 变更触发器
CREATE OR REPLACE FUNCTION log_experiment_change()
RETURNS TRIGGER AS $$
BEGIN
    IF (TG_OP = 'DELETE') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('experiment', OLD.eid::text, 'delete');
        RETURN OLD;
    ELSIF (TG_OP = 'UPDATE') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('experiment', NEW.eid::text, 'update');
        RETURN NEW;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO config_change_log (entity_type, entity_id, operation)
        VALUES ('experiment', NEW.eid::text, 'create');
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER log_experiment_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON experiments
    FOR EACH ROW EXECUTE FUNCTION log_experiment_change();

-- 清理旧流水（保留 7 天）
COMMENT ON TABLE config_change_log IS '配置变更流水表，控制面通过轮询 id 感知变更。定期清理 7 天前数据';


-- ============================================
-- 配置变更通知触发器（LISTEN/NOTIFY）
-- ============================================

-- Layer 变更通知函数
CREATE OR REPLACE FUNCTION notify_layer_change()
RETURNS TRIGGER AS $$
DECLARE
    payload JSON;
BEGIN
    IF (TG_OP = 'DELETE') THEN
        payload = json_build_object(
            'operation', 'DELETE',
            'table', 'layers',
            'layer_id', OLD.layer_id,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN OLD;
    ELSIF (TG_OP = 'UPDATE') THEN
        payload = json_build_object(
            'operation', 'UPDATE',
            'table', 'layers',
            'layer_id', NEW.layer_id,
            'enabled', NEW.enabled,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN NEW;
    ELSIF (TG_OP = 'INSERT') THEN
        payload = json_build_object(
            'operation', 'INSERT',
            'table', 'layers',
            'layer_id', NEW.layer_id,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Experiment 变更通知函数
CREATE OR REPLACE FUNCTION notify_experiment_change()
RETURNS TRIGGER AS $$
DECLARE
    payload JSON;
BEGIN
    IF (TG_OP = 'DELETE') THEN
        payload = json_build_object(
            'operation', 'DELETE',
            'table', 'experiments',
            'eid', OLD.eid,
            'service', OLD.service,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN OLD;
    ELSIF (TG_OP = 'UPDATE') THEN
        payload = json_build_object(
            'operation', 'UPDATE',
            'table', 'experiments',
            'eid', NEW.eid,
            'service', NEW.service,
            'status', NEW.status,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN NEW;
    ELSIF (TG_OP = 'INSERT') THEN
        payload = json_build_object(
            'operation', 'INSERT',
            'table', 'experiments',
            'eid', NEW.eid,
            'service', NEW.service,
            'timestamp', extract(epoch from now())::bigint
        );
        PERFORM pg_notify('config_changes', payload::text);
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- 创建触发器
CREATE TRIGGER layer_change_notify
    AFTER INSERT OR UPDATE OR DELETE ON layers
    FOR EACH ROW EXECUTE FUNCTION notify_layer_change();

CREATE TRIGGER experiment_change_notify
    AFTER INSERT OR UPDATE OR DELETE ON experiments
    FOR EACH ROW EXECUTE FUNCTION notify_experiment_change();
