package grpc_server

import (
	"context"
	"fmt"
	"sync"
	"time"

	"go.uber.org/zap"
)

// Configuration push server for data plane instances
type XDSServer struct {
	logger *zap.Logger

	// Client connection management
	mu      sync.RWMutex
	clients map[string]*ClientState

	// Configuration state
	configMu    sync.RWMutex
	layers      map[string]*Layer
	experiments map[string]*Experiment

	// Version tracking
	versionCounter int64
}

// Layer configuration
type Layer struct {
	LayerId   string
	Version   string
	UpdatedAt time.Time
}

// Experiment configuration
type Experiment struct {
	Eid       int64
	Service   string
	UpdatedAt time.Time
}

// Connected client state
type ClientState struct {
	NodeID      string
	ConnectedAt time.Time
	LastSeen    time.Time
}

// Create new configuration push server
func NewXDSServer(logger *zap.Logger) *XDSServer {
	return &XDSServer{
		logger:      logger,
		clients:     make(map[string]*ClientState),
		layers:      make(map[string]*Layer),
		experiments: make(map[string]*Experiment),
	}
}

// Update layer configuration
func (s *XDSServer) UpdateLayer(layer *Layer) {
	s.configMu.Lock()
	defer s.configMu.Unlock()

	layer.UpdatedAt = time.Now()
	s.layers[layer.LayerId] = layer

	s.logger.Info("Layer updated",
		zap.String("layer_id", layer.LayerId),
		zap.String("version", layer.Version))
}

// Remove layer configuration
func (s *XDSServer) DeleteLayer(layerID string) {
	s.configMu.Lock()
	defer s.configMu.Unlock()

	delete(s.layers, layerID)
	s.logger.Info("Layer deleted", zap.String("layer_id", layerID))
}

// Update experiment configuration
func (s *XDSServer) UpdateExperiment(exp *Experiment) {
	s.configMu.Lock()
	defer s.configMu.Unlock()

	exp.UpdatedAt = time.Now()
	key := fmt.Sprintf("%s-%d", exp.Service, exp.Eid)
	s.experiments[key] = exp

	s.logger.Info("Experiment updated",
		zap.Int64("eid", exp.Eid),
		zap.String("service", exp.Service))
}

// Remove experiment configuration
func (s *XDSServer) DeleteExperiment(service string, eid int64) {
	s.configMu.Lock()
	defer s.configMu.Unlock()

	key := fmt.Sprintf("%s-%d", service, eid)
	delete(s.experiments, key)

	s.logger.Info("Experiment deleted",
		zap.Int64("eid", eid),
		zap.String("service", service))
}

// Get connected client count
func (s *XDSServer) GetClientCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.clients)
}

// Handle database change notifications
func (s *XDSServer) HandleDBChange(changeType string, entityID string) {
	s.logger.Info("DB change received",
		zap.String("type", changeType),
		zap.String("entity_id", entityID))
}

// Get subscriber count (alias for client count)
func (s *XDSServer) GetSubscriberCount() int {
	return s.GetClientCount()
}

// Type alias for backward compatibility
type PushServer = XDSServer

// Create new push server (alias)
func NewPushServer(logger *zap.Logger) *PushServer {
	return NewXDSServer(logger)
}
