package auth

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strconv"
	"time"
)

// AKSK authentication handler
type AKSKAuth struct {
	store AKSKStore
}

// Store interface for AKSK credentials
type AKSKStore interface {
	GetSecretKey(ctx context.Context, accessKey string) (string, error)
	GetServiceInfo(ctx context.Context, accessKey string) (*ServiceInfo, error)
}

// Service authentication information
type ServiceInfo struct {
	AccessKey   string
	SecretKey   string
	ServiceName string
	Permissions []string
	Status      string // active, disabled, etc.
	CreatedAt   time.Time
}

// Create new AKSK authentication handler
func NewAKSKAuth(store AKSKStore) *AKSKAuth {
	return &AKSKAuth{store: store}
}

// Generate HMAC signature for request
func (a *AKSKAuth) GenerateSignature(accessKey, secretKey, method, path string, timestamp int64, body []byte) string {
	// 构造待签名字符串
	stringToSign := fmt.Sprintf("%s\n%s\n%d\n%s", method, path, timestamp, string(body))
	
	// Generate HMAC-SHA256 signature
	h := hmac.New(sha256.New, []byte(secretKey))
	h.Write([]byte(stringToSign))
	
	return hex.EncodeToString(h.Sum(nil))
}

// Verify request signature and return service info
func (a *AKSKAuth) VerifySignature(ctx context.Context, accessKey, signature, timestampStr, method, path string, body []byte) (*ServiceInfo, error) {
	// 解析时间戳
	timestamp, err := strconv.ParseInt(timestampStr, 10, 64)
	if err != nil {
		return nil, fmt.Errorf("invalid timestamp: %w", err)
	}
	
	// 检查时间戳（防重放攻击）
	now := time.Now().Unix()
	if abs(now-timestamp) > 300 { // 5 minute window to prevent replay attacks
		return nil, fmt.Errorf("timestamp expired")
	}
	
	// 获取服务信息
	service, err := a.store.GetServiceInfo(ctx, accessKey)
	if err != nil {
		return nil, fmt.Errorf("get service info: %w", err)
	}
	
	if service.Status != "active" {
		return nil, fmt.Errorf("service disabled")
	}
	
	// 计算期望签名
	expectedSignature := a.GenerateSignature(accessKey, service.SecretKey, method, path, timestamp, body)
	
	// 比较签名
	if !hmac.Equal([]byte(signature), []byte(expectedSignature)) {
		return nil, fmt.Errorf("signature mismatch")
	}
	
	return service, nil
}

func abs(x int64) int64 {
	if x < 0 {
		return -x
	}
	return x
}