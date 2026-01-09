-- AK/SK 服务认证表
CREATE TABLE IF NOT EXISTS service_credentials (
    access_key VARCHAR(32) PRIMARY KEY,
    secret_key VARCHAR(64) NOT NULL,
    service_name VARCHAR(100) NOT NULL,
    service_type VARCHAR(20) NOT NULL, -- 'data_plane', 'api_client'
    permissions TEXT[], -- 权限列表
    status VARCHAR(10) NOT NULL DEFAULT 'active', -- 'active', 'disabled'
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

-- 索引
CREATE INDEX idx_service_credentials_service_name ON service_credentials(service_name);
CREATE INDEX idx_service_credentials_status ON service_credentials(status);
CREATE INDEX idx_service_credentials_type ON service_credentials(service_type);

-- 示例数据
INSERT INTO service_credentials (access_key, secret_key, service_name, service_type, permissions, description) VALUES
('AKID1234567890ABCDEF', 'sk_1234567890abcdef1234567890abcdef12345678', 'data-plane-001', 'data_plane', 
 ARRAY['config:read', 'experiment:read'], '数据面节点001'),
('AKID0987654321FEDCBA', 'sk_fedcba0987654321fedcba0987654321fedcba09', 'api-client-001', 'api_client', 
 ARRAY['layer:read', 'layer:write', 'experiment:read'], 'API客户端001');

-- 更新时间触发器
CREATE OR REPLACE FUNCTION update_service_credentials_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_service_credentials_updated_at
    BEFORE UPDATE ON service_credentials
    FOR EACH ROW
    EXECUTE FUNCTION update_service_credentials_updated_at();