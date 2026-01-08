package main

import (
	"context"
	"flag"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/georgeji/experiment-system/control-plane/internal/config"
	"github.com/georgeji/experiment-system/control-plane/internal/grpc_server"
	"github.com/georgeji/experiment-system/control-plane/internal/notifier"
	pb "github.com/georgeji/experiment-system/control-plane/proto"
	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
	"go.uber.org/zap"
	"google.golang.org/grpc"
)

var (
	configPath = flag.String("config", "config.yaml", "config file path")
)

func main() {
	flag.Parse()

	// 加载配置
	cfg, err := config.Load(*configPath)
	if err != nil {
		panic(fmt.Errorf("load config: %w", err))
	}

	// 初始化日志
	logger, err := initLogger(cfg.Log.Level)
	if err != nil {
		panic(fmt.Errorf("init logger: %w", err))
	}
	defer logger.Sync()

	logger.Info("starting control plane",
		zap.String("http_addr", fmt.Sprintf("%s:%d", cfg.Server.Host, cfg.Server.Port)),
		zap.String("grpc_addr", fmt.Sprintf("%s:%d", cfg.GRPC.Host, cfg.GRPC.Port)),
	)

	// 连接数据库
	dbpool, err := pgxpool.New(context.Background(), cfg.Database.DSN())
	if err != nil {
		logger.Fatal("connect to database failed", zap.Error(err))
	}
	defer dbpool.Close()

	logger.Info("database connected")

	// 启动 gRPC Server
	pushServer := grpc_server.NewPushServer(logger)
	grpcServer := grpc.NewServer()
	pb.RegisterConfigPushServiceServer(grpcServer, pushServer)

	grpcListener, err := net.Listen("tcp", fmt.Sprintf("%s:%d", cfg.GRPC.Host, cfg.GRPC.Port))
	if err != nil {
		logger.Fatal("grpc listen failed", zap.Error(err))
	}

	go func() {
		logger.Info("grpc server listening", zap.String("addr", grpcListener.Addr().String()))
		if err := grpcServer.Serve(grpcListener); err != nil {
			logger.Fatal("grpc serve failed", zap.Error(err))
		}
	}()

	// 启动 PostgreSQL LISTEN/NOTIFY
	pgNotifier := notifier.NewPgNotifier(dbpool, logger)
	pgNotifier.RegisterHandler(pushServer.HandleDBChange)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go func() {
		if err := pgNotifier.Start(ctx); err != nil && err != context.Canceled {
			logger.Error("pg notifier failed", zap.Error(err))
		}
	}()

	// 启动 HTTP Server
	router := gin.Default()
	setupRoutes(router, cfg, logger, pushServer)

	httpServer := &http.Server{
		Addr:    fmt.Sprintf("%s:%d", cfg.Server.Host, cfg.Server.Port),
		Handler: router,
	}

	go func() {
		logger.Info("http server listening", zap.String("addr", httpServer.Addr))
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			logger.Fatal("http serve failed", zap.Error(err))
		}
	}()

	// 优雅关闭
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	logger.Info("shutting down servers...")

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := httpServer.Shutdown(ctx); err != nil {
		logger.Error("http server shutdown error", zap.Error(err))
	}

	grpcServer.GracefulStop()
	logger.Info("servers stopped")
}

func setupRoutes(r *gin.Engine, cfg *config.Config, logger *zap.Logger, pushServer *grpc_server.PushServer) {
	// Health check
	r.GET("/health", func(c *gin.Context) {
		c.JSON(200, gin.H{
			"status":      "ok",
			"subscribers": pushServer.GetSubscriberCount(),
		})
	})

	// API v1
	v1 := r.Group("/api/v1")
	{
		// Auth
		auth := v1.Group("/auth")
		{
			auth.POST("/login", func(c *gin.Context) {
				// TODO: 实现登录
				c.JSON(200, gin.H{"message": "login endpoint"})
			})
			auth.POST("/register", func(c *gin.Context) {
				// TODO: 实现注册
				c.JSON(200, gin.H{"message": "register endpoint"})
			})
		}

		// OIDC
		oidc := r.Group("/.well-known")
		{
			oidc.GET("/openid-configuration", func(c *gin.Context) {
				c.JSON(200, gin.H{
					"issuer":                 cfg.OIDC.Issuer,
					"authorization_endpoint": cfg.OIDC.Issuer + "/oauth/authorize",
					"token_endpoint":         cfg.OIDC.Issuer + "/oauth/token",
					"userinfo_endpoint":      cfg.OIDC.Issuer + "/oauth/userinfo",
					"jwks_uri":               cfg.OIDC.Issuer + "/.well-known/jwks.json",
					"response_types_supported": []string{"code", "token"},
					"grant_types_supported":    []string{"authorization_code", "refresh_token"},
					"subject_types_supported":  []string{"public"},
					"id_token_signing_alg_values_supported": []string{"HS256"},
				})
			})
		}

		// Layers
		layers := v1.Group("/layers")
		{
			layers.GET("", func(c *gin.Context) {
				// TODO: 列出 Layers
				c.JSON(200, gin.H{"message": "list layers"})
			})
			layers.POST("", func(c *gin.Context) {
				// TODO: 创建 Layer
				c.JSON(201, gin.H{"message": "create layer"})
			})
			layers.PUT("/:id", func(c *gin.Context) {
				// TODO: 更新 Layer
				c.JSON(200, gin.H{"message": "update layer"})
			})
			layers.DELETE("/:id", func(c *gin.Context) {
				// TODO: 删除 Layer
				c.JSON(204, nil)
			})
		}

		// Experiments
		experiments := v1.Group("/experiments")
		{
			experiments.GET("", func(c *gin.Context) {
				// TODO: 列出 Experiments
				c.JSON(200, gin.H{"message": "list experiments"})
			})
			experiments.POST("", func(c *gin.Context) {
				// TODO: 创建 Experiment
				c.JSON(201, gin.H{"message": "create experiment"})
			})
			experiments.PUT("/:id", func(c *gin.Context) {
				// TODO: 更新 Experiment
				c.JSON(200, gin.H{"message": "update experiment"})
			})
			experiments.DELETE("/:id", func(c *gin.Context) {
				// TODO: 删除 Experiment
				c.JSON(204, nil)
			})
		}

		// Data Planes
		dataPlanes := v1.Group("/data-planes")
		{
			dataPlanes.GET("", func(c *gin.Context) {
				// TODO: 列出数据面实例
				c.JSON(200, gin.H{"message": "list data planes"})
			})
		}
	}
}

func initLogger(level string) (*zap.Logger, error) {
	config := zap.NewProductionConfig()
	config.Level = zap.NewAtomicLevelAt(parseLogLevel(level))
	return config.Build()
}

func parseLogLevel(level string) zap.AtomicLevel {
	switch level {
	case "debug":
		return zap.NewAtomicLevelAt(zap.DebugLevel)
	case "info":
		return zap.NewAtomicLevelAt(zap.InfoLevel)
	case "warn":
		return zap.NewAtomicLevelAt(zap.WarnLevel)
	case "error":
		return zap.NewAtomicLevelAt(zap.ErrorLevel)
	default:
		return zap.NewAtomicLevelAt(zap.InfoLevel)
	}
}
