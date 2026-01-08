package grpc_server

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/georgeji/experiment-system/control-plane/internal/state"
	pb "github.com/georgeji/experiment-system/control-plane/proto"
	"go.uber.org/zap"
)

// PushServer gRPC 推送服务器（Istio xDS 风格）
type PushServer struct {
	pb.UnimplementedConfigPushServiceServer
	logger      *zap.Logger
	state       *state.ConfigState // 内存状态
	subscribers sync.Map            // map[string]*Subscriber
	broadcast   chan *pb.ConfigChange
}

// Subscriber 订阅者
type Subscriber struct {
	ID       string
	Services []string
	Version  string
	Stream   pb.ConfigPushService_SubscribeConfigServer
	Updates  chan *pb.ConfigChange
	Done     chan struct{}
}

func NewPushServer(logger *zap.Logger, configState *state.ConfigState) *PushServer {
	s := &PushServer{
		logger:    logger,
		state:     configState,
		broadcast: make(chan *pb.ConfigChange, 100),
	}

	// 注册为 ConfigState 的变更监听器
	configState.RegisterChangeHandler(s.handleStateChange)

	go s.broadcastLoop()
	return s
}

// handleStateChange 处理内存状态变更（由 ConfigState 回调）
func (s *PushServer) handleStateChange(change *state.ConfigChange) {
	s.logger.Debug("handling state change",
		zap.Int("type", int(change.Type)),
		zap.Int64("version", change.Version),
	)

	var configChange *pb.ConfigChange

	switch change.Type {
	case state.LayerCreated, state.LayerUpdated:
		configChange = &pb.ConfigChange{
			Type:      pb.ConfigChange_LAYER_UPDATE,
			Version:   fmt.Sprintf("v%d", change.Version),
			Timestamp: change.Timestamp,
			Layers:    []*pb.Layer{
				// TODO: 转换模型
			},
		}
	case state.LayerDeleted:
		configChange = &pb.ConfigChange{
			Type:            pb.ConfigChange_LAYER_DELETE,
			Version:         fmt.Sprintf("v%d", change.Version),
			Timestamp:       change.Timestamp,
			DeletedLayerIds: []string{change.DeletedLayerID},
		}
	case state.ExperimentCreated, state.ExperimentUpdated:
		configChange = &pb.ConfigChange{
			Type:        pb.ConfigChange_EXPERIMENT_UPDATE,
			Version:     fmt.Sprintf("v%d", change.Version),
			Timestamp:   change.Timestamp,
			Experiments: []*pb.Experiment{
				// TODO: 转换模型
			},
		}
	case state.ExperimentDeleted:
		configChange = &pb.ConfigChange{
			Type:                 pb.ConfigChange_EXPERIMENT_DELETE,
			Version:              fmt.Sprintf("v%d", change.Version),
			Timestamp:            change.Timestamp,
			DeletedExperimentIds: []int32{change.DeletedEID},
		}
	}

	if configChange != nil {
		s.BroadcastChange(configChange)
	}
}

// SubscribeConfig 订阅配置变更
func (s *PushServer) SubscribeConfig(req *pb.SubscribeRequest, stream pb.ConfigPushService_SubscribeConfigServer) error {
	s.logger.Info("new subscriber",
		zap.String("data_plane_id", req.DataPlaneId),
		zap.String("version", req.Version),
		zap.Strings("services", req.Services),
	)

	sub := &Subscriber{
		ID:       req.DataPlaneId,
		Services: req.Services,
		Version:  req.Version,
		Stream:   stream,
		Updates:  make(chan *pb.ConfigChange, 10),
		Done:     make(chan struct{}),
	}

	s.subscribers.Store(req.DataPlaneId, sub)
	defer func() {
		s.subscribers.Delete(req.DataPlaneId)
		close(sub.Done)
		s.logger.Info("subscriber disconnected", zap.String("data_plane_id", req.DataPlaneId))
	}()

	// 发送当前完整配置
	if err := s.sendFullConfig(stream, req); err != nil {
		return fmt.Errorf("send full config: %w", err)
	}

	// 持续推送变更
	for {
		select {
		case <-stream.Context().Done():
			return stream.Context().Err()
		case change := <-sub.Updates:
			if err := stream.Send(change); err != nil {
				s.logger.Error("send change failed",
					zap.String("data_plane_id", req.DataPlaneId),
					zap.Error(err),
				)
				return err
			}
			s.logger.Debug("change sent",
				zap.String("data_plane_id", req.DataPlaneId),
				zap.String("type", change.Type.String()),
			)
		}
	}
}

// GetFullConfig 全量拉取配置（从内存读取）
func (s *PushServer) GetFullConfig(ctx context.Context, req *pb.GetFullConfigRequest) (*pb.FullConfig, error) {
	snapshot := s.state.GetFullSnapshot(req.Service)

	config := &pb.FullConfig{
		Version:     fmt.Sprintf("v%d", snapshot.Version),
		Timestamp:   snapshot.Timestamp,
		Layers:      snapshot.Layers,
		Experiments: snapshot.Experiments,
	}
	return config, nil
}

// HealthCheck 健康检查
func (s *PushServer) HealthCheck(ctx context.Context, req *pb.HealthCheckRequest) (*pb.HealthCheckResponse, error) {
	return &pb.HealthCheckResponse{
		Healthy:   true,
		Version:   "1.0.0",
		Timestamp: time.Now().Unix(),
	}, nil
}

// BroadcastChange 广播配置变更
func (s *PushServer) BroadcastChange(change *pb.ConfigChange) {
	select {
	case s.broadcast <- change:
	default:
		s.logger.Warn("broadcast channel full, dropping change")
	}
}

// broadcastLoop 广播循环
func (s *PushServer) broadcastLoop() {
	for change := range s.broadcast {
		s.subscribers.Range(func(key, value interface{}) bool {
			sub := value.(*Subscriber)
			
			// TODO: 根据 sub.Services 过滤变更
			
			select {
			case sub.Updates <- change:
			case <-sub.Done:
			default:
				s.logger.Warn("subscriber queue full",
					zap.String("data_plane_id", sub.ID),
				)
			}
			return true
		})
	}
}

// sendFullConfig 发送完整配置（从内存读取）
func (s *PushServer) sendFullConfig(stream pb.ConfigPushService_SubscribeConfigServer, req *pb.SubscribeRequest) error {
	// 从内存获取全量快照
	snapshot := s.state.GetFullSnapshot(req.Services[0]) // TODO: 支持多 service

	fullConfig := &pb.ConfigChange{
		Type:        pb.ConfigChange_FULL_RELOAD,
		Version:     fmt.Sprintf("v%d", snapshot.Version),
		Timestamp:   snapshot.Timestamp,
		Layers:      snapshot.Layers,
		Experiments: snapshot.Experiments,
	}

	return stream.Send(fullConfig)
}

// GetSubscriberCount 获取订阅者数量
func (s *PushServer) GetSubscriberCount() int {
	count := 0
	s.subscribers.Range(func(key, value interface{}) bool {
		count++
		return true
	})
	return count
}
