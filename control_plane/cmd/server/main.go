package main

import (
	"flag"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"syscall"

	"github.com/georgeji/experiment-system/control-plane/internal/config"
	"github.com/georgeji/experiment-system/control-plane/internal/grpc_server"
	"github.com/gin-gonic/gin"
	"go.uber.org/zap"
)

var (
	configPath = flag.String("config", "config.yaml", "config file path")
)

func main() {
	flag.Parse()

	// 初始化日志
	logger, _ := zap.NewProduction()
	defer logger.Sync()

	logger.Info("starting experiment control plane server")

	// 加载配置
	cfg, err := config.Load(*configPath)
	if err != nil {
		logger.Fatal("failed to load config", zap.Error(err))
	}

	logger.Info("config loaded",
		zap.String("server_addr", fmt.Sprintf("%s:%d", cfg.Server.Host, cfg.Server.Port)),
	)

	// Configuration and utility functions
	xdsServer := grpc_server.NewXDSServer(logger)

	// Configuration and utility functions
	router := gin.Default()
	setupRoutes(router, cfg, logger, xdsServer)

	httpServer := &http.Server{
		Addr:    fmt.Sprintf("%s:%d", cfg.Server.Host, cfg.Server.Port),
		Handler: router,
	}

	go func() {
		logger.Info("HTTP server listening", zap.String("addr", httpServer.Addr))
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			logger.Fatal("HTTP server failed", zap.Error(err))
		}
	}()

	// 优雅关闭
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	logger.Info("shutting down servers...")
	logger.Info("servers stopped")
}

func setupRoutes(r *gin.Engine, cfg *config.Config, logger *zap.Logger, xdsServer *grpc_server.XDSServer) {
	// Configuration and utility functions
	r.GET("/health", func(c *gin.Context) {
		c.JSON(200, gin.H{
			"status":  "ok",
			"message": "Experiment Control Plane",
			"clients": xdsServer.GetClientCount(),
		})
	})

	// Configuration and utility functions
	r.GET("/status", func(c *gin.Context) {
		c.JSON(200, gin.H{
			"version": "v1.0",
			"status":  "running",
			"clients": xdsServer.GetClientCount(),
		})
	})

	// 配置管理接口
	api := r.Group("/api/v1")
	{
		// Configuration and utility functions
		api.POST("/layers", func(c *gin.Context) {
			var layer grpc_server.Layer
			if err := c.ShouldBindJSON(&layer); err != nil {
				c.JSON(400, gin.H{"error": err.Error()})
				return
			}

			xdsServer.UpdateLayer(&layer)
			c.JSON(200, gin.H{"message": "Layer updated"})
		})

		api.DELETE("/layers/:id", func(c *gin.Context) {
			layerID := c.Param("id")
			xdsServer.DeleteLayer(layerID)
			c.JSON(200, gin.H{"message": "Layer deleted"})
		})

		// Configuration and utility functions
		api.POST("/experiments", func(c *gin.Context) {
			var exp grpc_server.Experiment
			if err := c.ShouldBindJSON(&exp); err != nil {
				c.JSON(400, gin.H{"error": err.Error()})
				return
			}

			xdsServer.UpdateExperiment(&exp)
			c.JSON(200, gin.H{"message": "Experiment updated"})
		})

		api.DELETE("/experiments/:service/:eid", func(c *gin.Context) {
			service := c.Param("service")
			eid := c.GetInt64("eid")

			xdsServer.DeleteExperiment(service, eid)
			c.JSON(200, gin.H{"message": "Experiment deleted"})
		})
	}
}
