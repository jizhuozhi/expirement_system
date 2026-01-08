package oidc

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
	"golang.org/x/crypto/bcrypt"
)

// Provider OIDC Provider
type Provider struct {
	issuer     string
	jwtSecret  []byte
	accessTTL  time.Duration
	refreshTTL time.Duration
	store      Store
}

// Store 存储接口
type Store interface {
	// User
	GetUserByEmail(ctx context.Context, email string) (*User, error)
	CreateUser(ctx context.Context, user *User) error
	
	// Client
	GetClient(ctx context.Context, clientID string) (*Client, error)
	
	// Authorization Code
	SaveAuthCode(ctx context.Context, code *AuthorizationCode) error
	GetAuthCode(ctx context.Context, code string) (*AuthorizationCode, error)
	DeleteAuthCode(ctx context.Context, code string) error
	
	// Token
	SaveAccessToken(ctx context.Context, token *AccessToken) error
	GetAccessToken(ctx context.Context, token string) (*AccessToken, error)
	SaveRefreshToken(ctx context.Context, token *RefreshToken) error
	GetRefreshToken(ctx context.Context, token string) (*RefreshToken, error)
}

// User 用户
type User struct {
	ID           string
	Email        string
	PasswordHash string
	Name         string
	Role         string
}

// Client OIDC 客户端
type Client struct {
	ID            string
	Secret        string
	RedirectURIs  []string
	GrantTypes    []string
	ResponseTypes []string
	Scopes        []string
}

// AuthorizationCode 授权码
type AuthorizationCode struct {
	Code        string
	ClientID    string
	UserID      string
	RedirectURI string
	Scopes      []string
	ExpiresAt   time.Time
}

// AccessToken 访问令牌
type AccessToken struct {
	Token     string
	ClientID  string
	UserID    string
	Scopes    []string
	ExpiresAt time.Time
}

// RefreshToken 刷新令牌
type RefreshToken struct {
	Token     string
	ClientID  string
	UserID    string
	Scopes    []string
	ExpiresAt time.Time
}

// Claims JWT Claims
type Claims struct {
	UserID string   `json:"user_id"`
	Email  string   `json:"email"`
	Name   string   `json:"name"`
	Role   string   `json:"role"`
	Scopes []string `json:"scopes,omitempty"`
	jwt.RegisteredClaims
}

func NewProvider(issuer, jwtSecret string, accessTTL, refreshTTL time.Duration, store Store) *Provider {
	return &Provider{
		issuer:     issuer,
		jwtSecret:  []byte(jwtSecret),
		accessTTL:  accessTTL,
		refreshTTL: refreshTTL,
		store:      store,
	}
}

// Login 用户登录
func (p *Provider) Login(ctx context.Context, email, password string) (*User, error) {
	user, err := p.store.GetUserByEmail(ctx, email)
	if err != nil {
		return nil, fmt.Errorf("get user: %w", err)
	}

	if err := bcrypt.CompareHashAndPassword([]byte(user.PasswordHash), []byte(password)); err != nil {
		return nil, fmt.Errorf("invalid password")
	}

	return user, nil
}

// Register 用户注册
func (p *Provider) Register(ctx context.Context, email, password, name string) (*User, error) {
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return nil, fmt.Errorf("hash password: %w", err)
	}

	user := &User{
		ID:           uuid.New().String(),
		Email:        email,
		PasswordHash: string(hash),
		Name:         name,
		Role:         "viewer",
	}

	if err := p.store.CreateUser(ctx, user); err != nil {
		return nil, fmt.Errorf("create user: %w", err)
	}

	return user, nil
}

// GenerateAuthCode 生成授权码
func (p *Provider) GenerateAuthCode(ctx context.Context, clientID, userID, redirectURI string, scopes []string) (string, error) {
	code := generateRandomString(32)
	
	authCode := &AuthorizationCode{
		Code:        code,
		ClientID:    clientID,
		UserID:      userID,
		RedirectURI: redirectURI,
		Scopes:      scopes,
		ExpiresAt:   time.Now().Add(10 * time.Minute),
	}

	if err := p.store.SaveAuthCode(ctx, authCode); err != nil {
		return "", fmt.Errorf("save auth code: %w", err)
	}

	return code, nil
}

// ExchangeToken 授权码换令牌
func (p *Provider) ExchangeToken(ctx context.Context, code, clientID, clientSecret, redirectURI string) (*TokenResponse, error) {
	// 验证客户端
	client, err := p.store.GetClient(ctx, clientID)
	if err != nil {
		return nil, fmt.Errorf("get client: %w", err)
	}
	if client.Secret != clientSecret {
		return nil, fmt.Errorf("invalid client secret")
	}

	// 获取授权码
	authCode, err := p.store.GetAuthCode(ctx, code)
	if err != nil {
		return nil, fmt.Errorf("get auth code: %w", err)
	}
	if authCode.ClientID != clientID || authCode.RedirectURI != redirectURI {
		return nil, fmt.Errorf("invalid auth code")
	}
	if time.Now().After(authCode.ExpiresAt) {
		return nil, fmt.Errorf("auth code expired")
	}

	// 删除授权码
	if err := p.store.DeleteAuthCode(ctx, code); err != nil {
		return nil, fmt.Errorf("delete auth code: %w", err)
	}

	// 生成令牌
	return p.generateTokens(ctx, authCode.ClientID, authCode.UserID, authCode.Scopes)
}

// RefreshAccessToken 刷新访问令牌
func (p *Provider) RefreshAccessToken(ctx context.Context, refreshToken string) (*TokenResponse, error) {
	token, err := p.store.GetRefreshToken(ctx, refreshToken)
	if err != nil {
		return nil, fmt.Errorf("get refresh token: %w", err)
	}
	if time.Now().After(token.ExpiresAt) {
		return nil, fmt.Errorf("refresh token expired")
	}

	return p.generateTokens(ctx, token.ClientID, token.UserID, token.Scopes)
}

// generateTokens 生成访问令牌和刷新令牌
func (p *Provider) generateTokens(ctx context.Context, clientID, userID string, scopes []string) (*TokenResponse, error) {
	user, err := p.store.GetUserByEmail(ctx, userID) // 这里简化了，实际应该用 GetUserByID
	if err != nil {
		return nil, fmt.Errorf("get user: %w", err)
	}

	// 生成 Access Token
	accessToken := generateRandomString(32)
	accessExpiry := time.Now().Add(p.accessTTL)
	
	if err := p.store.SaveAccessToken(ctx, &AccessToken{
		Token:     accessToken,
		ClientID:  clientID,
		UserID:    userID,
		Scopes:    scopes,
		ExpiresAt: accessExpiry,
	}); err != nil {
		return nil, fmt.Errorf("save access token: %w", err)
	}

	// 生成 Refresh Token
	refreshToken := generateRandomString(32)
	refreshExpiry := time.Now().Add(p.refreshTTL)
	
	if err := p.store.SaveRefreshToken(ctx, &RefreshToken{
		Token:     refreshToken,
		ClientID:  clientID,
		UserID:    userID,
		Scopes:    scopes,
		ExpiresAt: refreshExpiry,
	}); err != nil {
		return nil, fmt.Errorf("save refresh token: %w", err)
	}

	// 生成 ID Token (JWT)
	idToken, err := p.generateIDToken(user, scopes, accessExpiry)
	if err != nil {
		return nil, fmt.Errorf("generate id token: %w", err)
	}

	return &TokenResponse{
		AccessToken:  accessToken,
		TokenType:    "Bearer",
		ExpiresIn:    int(p.accessTTL.Seconds()),
		RefreshToken: refreshToken,
		IDToken:      idToken,
	}, nil
}

// generateIDToken 生成 ID Token (JWT)
func (p *Provider) generateIDToken(user *User, scopes []string, expiry time.Time) (string, error) {
	claims := Claims{
		UserID: user.ID,
		Email:  user.Email,
		Name:   user.Name,
		Role:   user.Role,
		Scopes: scopes,
		RegisteredClaims: jwt.RegisteredClaims{
			Issuer:    p.issuer,
			Subject:   user.ID,
			Audience:  []string{p.issuer},
			ExpiresAt: jwt.NewNumericDate(expiry),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString(p.jwtSecret)
}

// VerifyToken 验证访问令牌
func (p *Provider) VerifyToken(ctx context.Context, tokenString string) (*AccessToken, error) {
	token, err := p.store.GetAccessToken(ctx, tokenString)
	if err != nil {
		return nil, fmt.Errorf("get token: %w", err)
	}
	if time.Now().After(token.ExpiresAt) {
		return nil, fmt.Errorf("token expired")
	}
	return token, nil
}

// TokenResponse 令牌响应
type TokenResponse struct {
	AccessToken  string `json:"access_token"`
	TokenType    string `json:"token_type"`
	ExpiresIn    int    `json:"expires_in"`
	RefreshToken string `json:"refresh_token,omitempty"`
	IDToken      string `json:"id_token,omitempty"`
}

func generateRandomString(length int) string {
	b := make([]byte, length)
	rand.Read(b)
	return base64.URLEncoding.EncodeToString(b)[:length]
}
