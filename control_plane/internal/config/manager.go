package config

import (
	"context"
	"sync"
	"time"

	"github.com/georgeji/experiment-system/control-plane/internal/grpc_server"
	"github.com/georgeji/experiment-system/control-plane/internal/repository"
	"go.uber.org/zap"
)

// Configuration change manager with polling
type ConfigManager struct {
	logger     *zap.Logger
	repo       repository.Repository
	xdsServer  *grpc_server.XDSServer
	
	// 轮询状态
	mu               sync.RWMutex
	lastChangeLogID  int64
	pollingInterval  time.Duration
	stopChan         chan struct{}
	running          bool
}

// Create new configuration manager
func NewConfigManager(
	logger *zap.Logger,
	repo repository.Repository,
	xdsServer *grpc_server.XDSServer,
) *ConfigManager {
	return &ConfigManager{
		logger:          logger,
		repo:           repo,
		xdsServer:      xdsServer,
		pollingInterval: 5 * time.Second,
		stopChan:       make(chan struct{}),
	}
}

// Start configuration polling
func (cm *ConfigManager) Start(ctx context.Context) error {
	cm.mu.Lock()
	if cm.running {
		cm.mu.Unlock()
		return nil
	}
	cm.running = true
	cm.mu.Unlock()

	// Initialize from latest changelog
	latestID, err := cm.repo.GetLatestChangeLogID(ctx)
	if err != nil {
		cm.logger.Warn("Failed to get latest changelog ID", zap.Error(err))
		latestID = 0
	}
	cm.lastChangeLogID = latestID

	// Start polling goroutine
	go cm.pollChanges(ctx)

	cm.logger.Info("Config manager started",
		zap.Int64("last_changelog_id", cm.lastChangeLogID),
		zap.Duration("polling_interval", cm.pollingInterval))

	return nil
}

// Stop configuration polling
func (cm *ConfigManager) Stop() {
	cm.mu.Lock()
	defer cm.mu.Unlock()

	if !cm.running {
		return
	}

	close(cm.stopChan)
	cm.running = false
	cm.logger.Info("Config manager stopped")
}

// Poll for configuration changes
func (cm *ConfigManager) pollChanges(ctx context.Context) {
	ticker := time.NewTicker(cm.pollingInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-cm.stopChan:
			return
		case <-ticker.C:
			if err := cm.checkChanges(ctx); err != nil {
				cm.logger.Error("Failed to check changes", zap.Error(err))
			}
		}
	}
}

// Check for new changes in changelog
func (cm *ConfigManager) checkChanges(ctx context.Context) error {
	changes, err := cm.repo.GetChangeLogAfter(ctx, cm.lastChangeLogID, 100)
	if err != nil {
		return err
	}

	if len(changes) == 0 {
		return nil
	}

	cm.logger.Debug("Processing changes", zap.Int("count", len(changes)))

	for _, change := range changes {
		cm.handleChange(change)
		cm.lastChangeLogID = change.ID
	}

	return nil
}

// Handle individual change log entry
func (cm *ConfigManager) handleChange(change *repository.ChangeLogEntry) {
	cm.logger.Info("Handling change",
		zap.Int64("id", change.ID),
		zap.String("entity_type", change.EntityType),
		zap.String("entity_id", change.EntityID),
		zap.String("operation", change.Operation))

	// Notify XDS server of change
	cm.xdsServer.HandleDBChange(change.Operation, change.EntityID)
}

// Get manager status information
func (cm *ConfigManager) GetStatus() map[string]interface{} {
	cm.mu.RLock()
	defer cm.mu.RUnlock()

	return map[string]interface{}{
		"running":            cm.running,
		"last_changelog_id":  cm.lastChangeLogID,
		"polling_interval":   cm.pollingInterval.String(),
		"xds_clients":        cm.xdsServer.GetClientCount(),
	}
}