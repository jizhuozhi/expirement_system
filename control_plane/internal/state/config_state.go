package state

import (
	"context"
	"fmt"
	"strconv"
	"sync"
	"time"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
	"github.com/georgeji/experiment-system/control-plane/internal/repository"
	"github.com/georgeji/experiment-system/control-plane/internal/sync"
	pb "github.com/georgeji/experiment-system/control-plane/proto"
	"go.uber.org/zap"
)

// ConfigState 控制面配置内存状态（流水表轮询方案）
type ConfigState struct {
	mu sync.RWMutex

	// 内存缓存
	layers      map[string]*models.Layer      // layer_id -> Layer
	experiments map[int32]*models.Experiment  // eid -> Experiment
	version     int64                         // 全局版本号

	// 依赖
	repo   repository.Repository
	logger *zap.Logger

	// 本地订阅者（gRPC 推送）
	changeHandlers []ChangeHandler
}

// ChangeHandler 配置变更回调
type ChangeHandler func(change *ConfigChange)

// ConfigChange 配置变更事件
type ConfigChange struct {
	Type      ChangeType
	Version   int64
	Timestamp int64

	// 变更内容
	Layer      *models.Layer
	Experiment *models.Experiment

	// 删除
	DeletedLayerID string
	DeletedEID     int32
}

type ChangeType int

const (
	LayerCreated ChangeType = iota
	LayerUpdated
	LayerDeleted
	ExperimentCreated
	ExperimentUpdated
	ExperimentDeleted
)

// NewConfigState 创建配置状态管理器
func NewConfigState(repo repository.Repository, logger *zap.Logger) *ConfigState {
	return &ConfigState{
		layers:         make(map[string]*models.Layer),
		experiments:    make(map[int32]*models.Experiment),
		repo:           repo,
		logger:         logger,
		changeHandlers: []ChangeHandler{},
	}
}

// LoadFromDB 从数据库加载全量配置（启动时调用）
func (s *ConfigState) LoadFromDB(ctx context.Context) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	start := time.Now()

	// 加载 Layers
	layers, err := s.repo.ListLayers(ctx, repository.ListLayersParams{})
	if err != nil {
		return err
	}
	for _, layer := range layers {
		s.layers[layer.LayerID] = layer
	}

	// 加载 Experiments
	experiments, err := s.repo.ListExperiments(ctx, repository.ListExperimentsParams{})
	if err != nil {
		return err
	}
	for _, exp := range experiments {
		s.experiments[exp.EID] = exp
	}

	s.version = time.Now().Unix()

	s.logger.Info("config loaded from db",
		zap.Int("layers", len(s.layers)),
		zap.Int("experiments", len(s.experiments)),
		zap.Duration("duration", time.Since(start)),
		zap.Int64("version", s.version),
	)

	return nil
}

// RegisterChangeHandler 注册变更监听器（PushServer 调用）
func (s *ConfigState) RegisterChangeHandler(handler ChangeHandler) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.changeHandlers = append(s.changeHandlers, handler)
}

// notifyLocalSubscribers 触发本地订阅者变更通知（推送给本节点的数据面）
func (s *ConfigState) notifyLocalSubscribers(change *ConfigChange) {
	for _, handler := range s.changeHandlers {
		go handler(change) // 异步通知，避免阻塞
	}
}

// HandleChangeLog 处理流水表变更（由 ChangeLogPoller 调用）
func (s *ConfigState) HandleChangeLog(entry *sync.ChangeLogEntry) error {
	s.logger.Debug("handling change log",
		zap.Int64("id", entry.ID),
		zap.String("entity_type", entry.EntityType),
		zap.String("operation", entry.Operation),
		zap.String("entity_id", entry.EntityID),
	)

	ctx := context.Background()

	switch entry.EntityType {
	case "layer":
		return s.handleLayerChange(ctx, entry)
	case "experiment":
		return s.handleExperimentChange(ctx, entry)
	default:
		return fmt.Errorf("unknown entity type: %s", entry.EntityType)
	}
}

// handleLayerChange 处理 Layer 变更
func (s *ConfigState) handleLayerChange(ctx context.Context, entry *sync.ChangeLogEntry) error {
	switch entry.Operation {
	case "create", "update":
		// 从数据库反查完整数据
		layer, err := s.repo.GetLayer(ctx, entry.EntityID)
		if err != nil {
			return fmt.Errorf("load layer: %w", err)
		}

		// 更新内存
		s.mu.Lock()
		s.layers[layer.LayerID] = layer
		s.version++
		version := s.version
		s.mu.Unlock()

		// 推送给本节点的数据面
		changeType := LayerCreated
		if entry.Operation == "update" {
			changeType = LayerUpdated
		}
		s.notifyLocalSubscribers(&ConfigChange{
			Type:      changeType,
			Version:   version,
			Timestamp: entry.CreatedAt.Unix(),
			Layer:     layer,
		})

	case "delete":
		s.mu.Lock()
		delete(s.layers, entry.EntityID)
		s.version++
		version := s.version
		s.mu.Unlock()

		s.notifyLocalSubscribers(&ConfigChange{
			Type:           LayerDeleted,
			Version:        version,
			Timestamp:      entry.CreatedAt.Unix(),
			DeletedLayerID: entry.EntityID,
		})
	}

	return nil
}

// handleExperimentChange 处理 Experiment 变更
func (s *ConfigState) handleExperimentChange(ctx context.Context, entry *sync.ChangeLogEntry) error {
	eid, err := strconv.ParseInt(entry.EntityID, 10, 32)
	if err != nil {
		return fmt.Errorf("parse eid: %w", err)
	}
	eid32 := int32(eid)

	switch entry.Operation {
	case "create", "update":
		// 从数据库反查完整数据
		exp, err := s.repo.GetExperiment(ctx, eid32)
		if err != nil {
			return fmt.Errorf("load experiment: %w", err)
		}

		s.mu.Lock()
		s.experiments[exp.EID] = exp
		s.version++
		version := s.version
		s.mu.Unlock()

		changeType := ExperimentCreated
		if entry.Operation == "update" {
			changeType = ExperimentUpdated
		}
		s.notifyLocalSubscribers(&ConfigChange{
			Type:       changeType,
			Version:    version,
			Timestamp:  entry.CreatedAt.Unix(),
			Experiment: exp,
		})

	case "delete":
		s.mu.Lock()
		delete(s.experiments, eid32)
		s.version++
		version := s.version
		s.mu.Unlock()

		s.notifyLocalSubscribers(&ConfigChange{
			Type:       ExperimentDeleted,
			Version:    version,
			Timestamp:  entry.CreatedAt.Unix(),
			DeletedEID: eid32,
		})
	}

	return nil
}

// ============================================
// Layer 操作（CRUD + 推送）
// ============================================

// CreateLayer 创建 Layer（写 DB，触发器自动写流水表）
func (s *ConfigState) CreateLayer(ctx context.Context, layer *models.Layer) error {
	// 1. 写数据库（触发器自动记录到 config_change_log）
	if err := s.repo.CreateLayer(ctx, layer); err != nil {
		return err
	}

	// 2. 更新本地内存
	s.mu.Lock()
	s.layers[layer.LayerID] = layer
	s.version++
	version := s.version
	s.mu.Unlock()

	// 3. 推送给本节点的数据面订阅者
	s.notifyLocalSubscribers(&ConfigChange{
		Type:      LayerCreated,
		Version:   version,
		Timestamp: time.Now().Unix(),
		Layer:     layer,
	})

	s.logger.Info("layer created",
		zap.String("layer_id", layer.LayerID),
		zap.Int64("version", version),
	)

	return nil
}

// UpdateLayer 更新 Layer
func (s *ConfigState) UpdateLayer(ctx context.Context, layer *models.Layer) error {
	if err := s.repo.UpdateLayer(ctx, layer); err != nil {
		return err
	}

	s.mu.Lock()
	s.layers[layer.LayerID] = layer
	s.version++
	version := s.version
	s.mu.Unlock()

	s.notifyLocalSubscribers(&ConfigChange{
		Type:      LayerUpdated,
		Version:   version,
		Timestamp: time.Now().Unix(),
		Layer:     layer,
	})

	s.logger.Info("layer updated",
		zap.String("layer_id", layer.LayerID),
		zap.Int64("version", version),
	)

	return nil
}

// DeleteLayer 删除 Layer
func (s *ConfigState) DeleteLayer(ctx context.Context, layerID string) error {
	if err := s.repo.DeleteLayer(ctx, layerID); err != nil {
		return err
	}

	s.mu.Lock()
	delete(s.layers, layerID)
	s.version++
	version := s.version
	s.mu.Unlock()

	s.notifyLocalSubscribers(&ConfigChange{
		Type:           LayerDeleted,
		Version:        version,
		Timestamp:      time.Now().Unix(),
		DeletedLayerID: layerID,
	})

	s.logger.Info("layer deleted",
		zap.String("layer_id", layerID),
		zap.Int64("version", version),
	)

	return nil
}

// GetLayer 读取 Layer（内存零拷贝）
func (s *ConfigState) GetLayer(layerID string) (*models.Layer, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	layer, ok := s.layers[layerID]
	return layer, ok
}

// ListLayers 列出所有 Layers（内存读取）
func (s *ConfigState) ListLayers(service string) []*models.Layer {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*models.Layer
	for _, layer := range s.layers {
		if service == "" || layer.Service == service {
			result = append(result, layer)
		}
	}
	return result
}

// ============================================
// Experiment 操作（CRUD + 推送）
// ============================================

// CreateExperiment 创建实验
func (s *ConfigState) CreateExperiment(ctx context.Context, exp *models.Experiment) error {
	if err := s.repo.CreateExperiment(ctx, exp); err != nil {
		return err
	}

	s.mu.Lock()
	s.experiments[exp.EID] = exp
	s.version++
	version := s.version
	s.mu.Unlock()

	s.notifyLocalSubscribers(&ConfigChange{
		Type:       ExperimentCreated,
		Version:    version,
		Timestamp:  time.Now().Unix(),
		Experiment: exp,
	})

	s.logger.Info("experiment created",
		zap.Int32("eid", exp.EID),
		zap.Int64("version", version),
	)

	return nil
}

// UpdateExperiment 更新实验
func (s *ConfigState) UpdateExperiment(ctx context.Context, exp *models.Experiment) error {
	if err := s.repo.UpdateExperiment(ctx, exp); err != nil {
		return err
	}

	s.mu.Lock()
	s.experiments[exp.EID] = exp
	s.version++
	version := s.version
	s.mu.Unlock()

	s.notifyLocalSubscribers(&ConfigChange{
		Type:       ExperimentUpdated,
		Version:    version,
		Timestamp:  time.Now().Unix(),
		Experiment: exp,
	})

	s.logger.Info("experiment updated",
		zap.Int32("eid", exp.EID),
		zap.Int64("version", version),
	)

	return nil
}

// DeleteExperiment 删除实验
func (s *ConfigState) DeleteExperiment(ctx context.Context, eid int32) error {
	if err := s.repo.DeleteExperiment(ctx, eid); err != nil {
		return err
	}

	s.mu.Lock()
	delete(s.experiments, eid)
	s.version++
	version := s.version
	s.mu.Unlock()

	s.notifyLocalSubscribers(&ConfigChange{
		Type:       ExperimentDeleted,
		Version:    version,
		Timestamp:  time.Now().Unix(),
		DeletedEID: eid,
	})

	s.logger.Info("experiment deleted",
		zap.Int32("eid", eid),
		zap.Int64("version", version),
	)

	return nil
}

// GetExperiment 读取实验
func (s *ConfigState) GetExperiment(eid int32) (*models.Experiment, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	exp, ok := s.experiments[eid]
	return exp, ok
}

// ListExperiments 列出所有实验
func (s *ConfigState) ListExperiments(service string) []*models.Experiment {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*models.Experiment
	for _, exp := range s.experiments {
		if service == "" || exp.Service == service {
			result = append(result, exp)
		}
	}
	return result
}

// ============================================
// 全量快照（用于新订阅者）
// ============================================

// GetFullSnapshot 获取全量配置快照（新数据面实例连接时）
func (s *ConfigState) GetFullSnapshot(service string) *pb.ConfigSnapshot {
	s.mu.RLock()
	defer s.mu.RUnlock()

	snapshot := &pb.ConfigSnapshot{
		Version:     s.version,
		Timestamp:   time.Now().Unix(),
		Layers:      []*pb.Layer{},
		Experiments: []*pb.Experiment{},
	}

	// 转换 Layers
	for _, layer := range s.layers {
		if service == "" || layer.Service == service {
			snapshot.Layers = append(snapshot.Layers, convertLayerToProto(layer))
		}
	}

	// 转换 Experiments
	for _, exp := range s.experiments {
		if service == "" || exp.Service == service {
			snapshot.Experiments = append(snapshot.Experiments, convertExperimentToProto(exp))
		}
	}

	return snapshot
}

// GetCurrentVersion 获取当前版本号
func (s *ConfigState) GetCurrentVersion() int64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.version
}

// ============================================
// 模型转换（TODO: 移到单独的 converter 包）
// ============================================

func convertLayerToProto(layer *models.Layer) *pb.Layer {
	// TODO: 完整实现
	return &pb.Layer{
		LayerId:  layer.LayerID,
		Service:  layer.Service,
		Priority: layer.Priority,
		Enabled:  layer.Enabled,
	}
}

func convertExperimentToProto(exp *models.Experiment) *pb.Experiment {
	// TODO: 完整实现
	return &pb.Experiment{
		Eid:     exp.EID,
		Service: exp.Service,
		Name:    exp.Name,
		Status:  exp.Status,
	}
}
