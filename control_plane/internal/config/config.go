package config

import (
	"fmt"

	"github.com/spf13/viper"
)

type Config struct {
	Server   ServerConfig   `mapstructure:"server"`
	Database DatabaseConfig `mapstructure:"database"`
	OIDC     OIDCConfig     `mapstructure:"oidc"`
	GRPC     GRPCConfig     `mapstructure:"grpc"`
	Log      LogConfig      `mapstructure:"log"`
	Gossip   GossipConfig   `mapstructure:"gossip"`
}

type GossipConfig struct {
	NodeID   string   `mapstructure:"node_id"`
	BindAddr string   `mapstructure:"bind_addr"`
	BindPort int      `mapstructure:"bind_port"`
	Peers    []string `mapstructure:"peers"`
}

type ServerConfig struct {
	Host string `mapstructure:"host"`
	Port int    `mapstructure:"port"`
}

type DatabaseConfig struct {
	Host     string `mapstructure:"host"`
	Port     int    `mapstructure:"port"`
	User     string `mapstructure:"user"`
	Password string `mapstructure:"password"`
	Database string `mapstructure:"database"`
	SSLMode  string `mapstructure:"sslmode"`
}

type OIDCConfig struct {
	Issuer       string `mapstructure:"issuer"`
	JWTSecret    string `mapstructure:"jwt_secret"`
	AccessTTL    int    `mapstructure:"access_ttl"`    // 秒
	RefreshTTL   int    `mapstructure:"refresh_ttl"`   // 秒
}

type GRPCConfig struct {
	Host string `mapstructure:"host"`
	Port int    `mapstructure:"port"`
}

type LogConfig struct {
	Level string `mapstructure:"level"`
}

func Load(configPath string) (*Config, error) {
	viper.SetConfigFile(configPath)
	viper.AutomaticEnv()

	// 默认值
	viper.SetDefault("server.host", "0.0.0.0")
	viper.SetDefault("server.port", 8081)
	viper.SetDefault("database.sslmode", "disable")
	viper.SetDefault("oidc.access_ttl", 3600)
	viper.SetDefault("oidc.refresh_ttl", 86400)
	viper.SetDefault("grpc.host", "0.0.0.0")
	viper.SetDefault("grpc.port", 9091)
	viper.SetDefault("log.level", "info")
	viper.SetDefault("gossip.bind_addr", "0.0.0.0")
	viper.SetDefault("gossip.bind_port", 7946)

	if err := viper.ReadInConfig(); err != nil {
		return nil, fmt.Errorf("read config: %w", err)
	}

	var cfg Config
	if err := viper.Unmarshal(&cfg); err != nil {
		return nil, fmt.Errorf("unmarshal config: %w", err)
	}

	return &cfg, nil
}

func (c *DatabaseConfig) DSN() string {
	return fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=%s",
		c.Host, c.Port, c.User, c.Password, c.Database, c.SSLMode,
	)
}
