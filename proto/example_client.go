package main

// Envoy 风格的 xDS 客户端示例
// 基于 envoy.service.discovery.v3 API 设计
// 注意: 这只是示例代码，需要实际的 proto 生成代码才能编译

import (
	"context"
	"fmt"
	"io"
	"log"
	"sync"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/types/known/anypb"
	
	// 这些导入需要实际生成的 proto 代码
	// configv1 "github.com/georgeji/experiment-system/proto/config/v1"
)

// 资源类型 URL 常量（与 Envoy 风格保持一致）
const (
	LayerTypeURL      = "type.googleapis.com/experiment.config.v1.Layer"
	ExperimentTypeURL = "type.googleapis.com/experiment.config.v1.Experiment"
)

// XDSClient - Envoy 风格的 xDS 客户端
type XDSClient struct {
	conn   *grpc.ClientConn
	client configv1.ConfigDiscoveryServiceClient
	node   *configv1.Node
	
	// SotW 状态管理
	sotw struct {
		mu           sync.RWMutex
		versionInfo  map[string]string // typeURL -> version
		lastNonce    map[string]string // typeURL -> nonce
	}
	
	// Delta 状态管理
	delta struct {
		mu               sync.RWMutex
		resourceVersions map[string]map[string]string // typeURL -> resourceName -> version
		subscriptions    map[string]map[string]bool   // typeURL -> resourceName -> subscribed
	}
	
	// 配置缓存
	cache struct {
		mu        sync.RWMutex
		layers    map[string]*configv1.Layer
		experiments map[string]*configv1.Experiment
	}
}

// NewXDSClient 创建新的 xDS 客户端
func NewXDSClient(serverAddr string, node *configv1.Node) (*XDSClient, error) {
	conn, err := grpc.Dial(serverAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, fmt.Errorf("failed to connect: %v", err)
	}

	client := configv1.NewConfigDiscoveryServiceClient(conn)
	
	c := &XDSClient{
		conn:   conn,
		client: client,
		node:   node,
	}
	
	// 初始化状态
	c.sotw.versionInfo = make(map[string]string)
	c.sotw.lastNonce = make(map[string]string)
	c.delta.resourceVersions = make(map[string]map[string]string)
	c.delta.subscriptions = make(map[string]map[string]bool)
	c.cache.layers = make(map[string]*configv1.Layer)
	c.cache.experiments = make(map[string]*configv1.Experiment)
	
	return c, nil
}

// Close 关闭连接
func (c *XDSClient) Close() error {
	return c.conn.Close()
}

// ============================================================================
// State of the World (SotW) xDS Implementation
// ============================================================================

// SubscribeLayersSotW 使用 SotW xDS 订阅 Layer 资源
func (c *XDSClient) SubscribeLayersSotW(ctx context.Context, resourceNames []string) error {
	return c.subscribeSotW(ctx, LayerTypeURL, resourceNames)
}

// SubscribeExperimentsSotW 使用 SotW xDS 订阅 Experiment 资源
func (c *XDSClient) SubscribeExperimentsSotW(ctx context.Context, resourceNames []string) error {
	return c.subscribeSotW(ctx, ExperimentTypeURL, resourceNames)
}

// subscribeSotW SotW xDS 通用订阅实现
func (c *XDSClient) subscribeSotW(ctx context.Context, typeURL string, resourceNames []string) error {
	stream, err := c.client.StreamConfigs(ctx)
	if err != nil {
		return fmt.Errorf("failed to create SotW stream for %s: %v", typeURL, err)
	}
	defer stream.CloseSend()

	// 发送初始请求
	req := &configv1.DiscoveryRequest{
		VersionInfo:   c.getSotwVersion(typeURL),
		Node:          c.node,
		ResourceNames: resourceNames,
		TypeUrl:       typeURL,
		ResponseNonce: "",
	}

	if err := stream.Send(req); err != nil {
		return fmt.Errorf("failed to send initial SotW request: %v", err)
	}

	log.Printf("[SotW] Sent initial subscription for %s, resources: %v", typeURL, resourceNames)

	// 处理响应循环
	for {
		resp, err := stream.Recv()
		if err == io.EOF {
			log.Printf("[SotW] Stream closed for %s", typeURL)
			break
		}
		if err != nil {
			return fmt.Errorf("failed to receive SotW response: %v", err)
		}

		log.Printf("[SotW] Received response for %s: version=%s, nonce=%s, resources=%d",
			typeURL, resp.VersionInfo, resp.Nonce, len(resp.Resources))

		// 处理资源
		if err := c.processSotwResources(typeURL, resp.Resources); err != nil {
			// NACK - 发送错误响应
			nackReq := &configv1.DiscoveryRequest{
				VersionInfo:   c.getSotwVersion(typeURL), // 保持旧版本
				Node:          c.node,
				ResourceNames: resourceNames,
				TypeUrl:       typeURL,
				ResponseNonce: resp.Nonce,
				ErrorDetail: status.New(codes.InvalidArgument, err.Error()).Proto(),
			}
			
			if err := stream.Send(nackReq); err != nil {
				return fmt.Errorf("failed to send SotW NACK: %v", err)
			}
			log.Printf("[SotW] Sent NACK for %s version %s: %v", typeURL, resp.VersionInfo, err)
		} else {
			// ACK - 确认接收
			ackReq := &configv1.DiscoveryRequest{
				VersionInfo:   resp.VersionInfo,
				Node:          c.node,
				ResourceNames: resourceNames,
				TypeUrl:       typeURL,
				ResponseNonce: resp.Nonce,
			}
			
			if err := stream.Send(ackReq); err != nil {
				return fmt.Errorf("failed to send SotW ACK: %v", err)
			}
			
			// 更新状态
			c.setSotwVersion(typeURL, resp.VersionInfo)
			c.setSotwNonce(typeURL, resp.Nonce)
			
			log.Printf("[SotW] Sent ACK for %s version %s", typeURL, resp.VersionInfo)
		}
	}

	return nil
}

// ============================================================================
// Delta xDS Implementation
// ============================================================================

// SubscribeLayersDelta 使用 Delta xDS 订阅 Layer 资源
func (c *XDSClient) SubscribeLayersDelta(ctx context.Context, layerNames []string) error {
	return c.subscribeDelta(ctx, LayerTypeURL, layerNames)
}

// SubscribeExperimentsDelta 使用 Delta xDS 订阅 Experiment 资源
func (c *XDSClient) SubscribeExperimentsDelta(ctx context.Context, experimentNames []string) error {
	return c.subscribeDelta(ctx, ExperimentTypeURL, experimentNames)
}

// subscribeDelta Delta xDS 通用订阅实现
func (c *XDSClient) subscribeDelta(ctx context.Context, typeURL string, resourceNames []string) error {
	stream, err := c.client.DeltaConfigs(ctx)
	if err != nil {
		return fmt.Errorf("failed to create Delta stream for %s: %v", typeURL, err)
	}
	defer stream.CloseSend()

	// 发送初始订阅请求
	req := &configv1.DeltaDiscoveryRequest{
		Node:                       c.node,
		TypeUrl:                    typeURL,
		ResourceNamesSubscribe:     resourceNames,
		InitialResourceVersions:    c.getDeltaResourceVersions(typeURL),
		ResourceVersions:           make(map[string]string),
	}

	if err := stream.Send(req); err != nil {
		return fmt.Errorf("failed to send initial Delta request: %v", err)
	}

	log.Printf("[Delta] Sent initial subscription for %s, resources: %v", typeURL, resourceNames)

	// 更新订阅状态
	c.updateDeltaSubscriptions(typeURL, resourceNames, true)

	// 处理响应循环
	for {
		resp, err := stream.Recv()
		if err == io.EOF {
			log.Printf("[Delta] Stream closed for %s", typeURL)
			break
		}
		if err != nil {
			return fmt.Errorf("failed to receive Delta response: %v", err)
		}

		log.Printf("[Delta] Received response for %s: nonce=%s, resources=%d, removed=%d",
			typeURL, resp.Nonce, len(resp.Resources), len(resp.RemovedResources))

		// 处理新增/更新的资源
		resourceVersions := c.getDeltaResourceVersions(typeURL)
		processingError := false
		
		for _, resource := range resp.Resources {
			if err := c.processDeltaResource(typeURL, resource); err != nil {
				// NACK
				nackReq := &configv1.DeltaDiscoveryRequest{
					Node:             c.node,
					TypeUrl:          typeURL,
					ResourceVersions: resourceVersions,
					ResponseNonce:    resp.Nonce,
					ErrorDetail: status.New(codes.InvalidArgument, 
						fmt.Sprintf("Failed to process resource %s: %v", resource.Name, err)).Proto(),
				}
				
				if err := stream.Send(nackReq); err != nil {
					return fmt.Errorf("failed to send Delta NACK: %v", err)
				}
				log.Printf("[Delta] Sent NACK for %s resource %s: %v", typeURL, resource.Name, err)
				processingError = true
				break
			}
			
			// 更新资源版本
			resourceVersions[resource.Name] = resource.Version
		}

		if processingError {
			continue
		}

		// 处理删除的资源
		for _, removedName := range resp.RemovedResources {
			c.removeDeltaResource(typeURL, removedName)
			delete(resourceVersions, removedName)
			log.Printf("[Delta] Removed resource: %s", removedName)
		}

		// 更新本地状态
		c.setDeltaResourceVersions(typeURL, resourceVersions)

		// ACK
		ackReq := &configv1.DeltaDiscoveryRequest{
			Node:             c.node,
			TypeUrl:          typeURL,
			ResourceVersions: resourceVersions,
			ResponseNonce:    resp.Nonce,
		}
		
		if err := stream.Send(ackReq); err != nil {
			return fmt.Errorf("failed to send Delta ACK: %v", err)
		}
		log.Printf("[Delta] Sent ACK for %s nonce %s", typeURL, resp.Nonce)
	}

	return nil
}

// ============================================================================
// Resource Processing
// ============================================================================

// processSotwResources 处理 SotW 模式的资源列表
func (c *XDSClient) processSotwResources(typeURL string, resources []*anypb.Any) error {
	switch typeURL {
	case LayerTypeURL:
		return c.processSotwLayers(resources)
	case ExperimentTypeURL:
		return c.processSotwExperiments(resources)
	default:
		return fmt.Errorf("unsupported resource type: %s", typeURL)
	}
}

// processSotwLayers 处理 Layer 资源列表
func (c *XDSClient) processSotwLayers(resources []*anypb.Any) error {
	newLayers := make(map[string]*configv1.Layer)
	
	for _, resource := range resources {
		var layer configv1.Layer
		if err := resource.UnmarshalTo(&layer); err != nil {
			return fmt.Errorf("failed to unmarshal layer: %v", err)
		}
		
		if err := c.validateLayer(&layer); err != nil {
			return fmt.Errorf("invalid layer %s: %v", layer.LayerId, err)
		}
		
		newLayers[layer.LayerId] = &layer
		log.Printf("[SotW] Processed layer: %s (version=%s, priority=%d)",
			layer.LayerId, layer.Version, layer.Priority)
	}
	
	// 原子更新缓存
	c.cache.mu.Lock()
	c.cache.layers = newLayers
	c.cache.mu.Unlock()
	
	return nil
}

// processSotwExperiments 处理 Experiment 资源列表
func (c *XDSClient) processSotwExperiments(resources []*anypb.Any) error {
	newExperiments := make(map[string]*configv1.Experiment)
	
	for _, resource := range resources {
		var experiment configv1.Experiment
		if err := resource.UnmarshalTo(&experiment); err != nil {
			return fmt.Errorf("failed to unmarshal experiment: %v", err)
		}
		
		if err := c.validateExperiment(&experiment); err != nil {
			return fmt.Errorf("invalid experiment %d: %v", experiment.Eid, err)
		}
		
		key := fmt.Sprintf("%s-%d", experiment.Service, experiment.Eid)
		newExperiments[key] = &experiment
		log.Printf("[SotW] Processed experiment: %d (service=%s, status=%s)",
			experiment.Eid, experiment.Service, experiment.Status)
	}
	
	// 原子更新缓存
	c.cache.mu.Lock()
	c.cache.experiments = newExperiments
	c.cache.mu.Unlock()
	
	return nil
}

// processDeltaResource 处理 Delta 模式的单个资源
func (c *XDSClient) processDeltaResource(typeURL string, resource *configv1.Resource) error {
	switch typeURL {
	case LayerTypeURL:
		return c.processDeltaLayer(resource)
	case ExperimentTypeURL:
		return c.processDeltaExperiment(resource)
	default:
		return fmt.Errorf("unsupported resource type: %s", typeURL)
	}
}

// processDeltaLayer 处理单个 Layer 资源
func (c *XDSClient) processDeltaLayer(resource *configv1.Resource) error {
	var layer configv1.Layer
	if err := resource.Resource.UnmarshalTo(&layer); err != nil {
		return fmt.Errorf("failed to unmarshal layer: %v", err)
	}
	
	if err := c.validateLayer(&layer); err != nil {
		return fmt.Errorf("invalid layer %s: %v", layer.LayerId, err)
	}
	
	// 更新缓存
	c.cache.mu.Lock()
	c.cache.layers[layer.LayerId] = &layer
	c.cache.mu.Unlock()
	
	log.Printf("[Delta] Processed layer: %s (version=%s, resource_version=%s)",
		layer.LayerId, layer.Version, resource.Version)
	return nil
}

// processDeltaExperiment 处理单个 Experiment 资源
func (c *XDSClient) processDeltaExperiment(resource *configv1.Resource) error {
	var experiment configv1.Experiment
	if err := resource.Resource.UnmarshalTo(&experiment); err != nil {
		return fmt.Errorf("failed to unmarshal experiment: %v", err)
	}
	
	if err := c.validateExperiment(&experiment); err != nil {
		return fmt.Errorf("invalid experiment %d: %v", experiment.Eid, err)
	}
	
	// 更新缓存
	key := fmt.Sprintf("%s-%d", experiment.Service, experiment.Eid)
	c.cache.mu.Lock()
	c.cache.experiments[key] = &experiment
	c.cache.mu.Unlock()
	
	log.Printf("[Delta] Processed experiment: %d (service=%s, resource_version=%s)",
		experiment.Eid, experiment.Service, resource.Version)
	return nil
}

// removeDeltaResource 删除资源
func (c *XDSClient) removeDeltaResource(typeURL, name string) {
	c.cache.mu.Lock()
	defer c.cache.mu.Unlock()
	
	switch typeURL {
	case LayerTypeURL:
		delete(c.cache.layers, name)
	case ExperimentTypeURL:
		delete(c.cache.experiments, name)
	}
	
	log.Printf("[Delta] Removed %s resource: %s", typeURL, name)
}

// ============================================================================
// Validation
// ============================================================================

// validateLayer 验证 Layer 配置
func (c *XDSClient) validateLayer(layer *configv1.Layer) error {
	if layer.LayerId == "" {
		return fmt.Errorf("layer_id is required")
	}
	
	if layer.HashKey == "" {
		return fmt.Errorf("hash_key is required")
	}
	
	// 验证 bucket ranges
	for i, r := range layer.Ranges {
		if r.Start >= r.End {
			return fmt.Errorf("invalid bucket range %d: start(%d) >= end(%d)", i, r.Start, r.End)
		}
		if r.End > 10000 {
			return fmt.Errorf("invalid bucket range %d: end(%d) > 10000", i, r.End)
		}
	}
	
	return nil
}

// validateExperiment 验证 Experiment 配置
func (c *XDSClient) validateExperiment(experiment *configv1.Experiment) error {
	if experiment.Eid == 0 {
		return fmt.Errorf("eid is required")
	}
	
	if experiment.Service == "" {
		return fmt.Errorf("service is required")
	}
	
	if len(experiment.Variants) == 0 {
		return fmt.Errorf("at least one variant is required")
	}
	
	// 验证变体
	for i, variant := range experiment.Variants {
		if variant.Vid == 0 {
			return fmt.Errorf("variant %d: vid is required", i)
		}
		if variant.Params == "" {
			return fmt.Errorf("variant %d: params is required", i)
		}
	}
	
	return nil
}

// ============================================================================
// State Management Helpers
// ============================================================================

func (c *XDSClient) getSotwVersion(typeURL string) string {
	c.sotw.mu.RLock()
	defer c.sotw.mu.RUnlock()
	return c.sotw.versionInfo[typeURL]
}

func (c *XDSClient) setSotwVersion(typeURL, version string) {
	c.sotw.mu.Lock()
	defer c.sotw.mu.Unlock()
	c.sotw.versionInfo[typeURL] = version
}

func (c *XDSClient) setSotwNonce(typeURL, nonce string) {
	c.sotw.mu.Lock()
	defer c.sotw.mu.Unlock()
	c.sotw.lastNonce[typeURL] = nonce
}

func (c *XDSClient) getDeltaResourceVersions(typeURL string) map[string]string {
	c.delta.mu.RLock()
	defer c.delta.mu.RUnlock()
	
	versions := c.delta.resourceVersions[typeURL]
	if versions == nil {
		return make(map[string]string)
	}
	
	// 返回副本
	result := make(map[string]string)
	for k, v := range versions {
		result[k] = v
	}
	return result
}

func (c *XDSClient) setDeltaResourceVersions(typeURL string, versions map[string]string) {
	c.delta.mu.Lock()
	defer c.delta.mu.Unlock()
	c.delta.resourceVersions[typeURL] = versions
}

func (c *XDSClient) updateDeltaSubscriptions(typeURL string, resourceNames []string, subscribed bool) {
	c.delta.mu.Lock()
	defer c.delta.mu.Unlock()
	
	if c.delta.subscriptions[typeURL] == nil {
		c.delta.subscriptions[typeURL] = make(map[string]bool)
	}
	
	for _, name := range resourceNames {
		c.delta.subscriptions[typeURL][name] = subscribed
	}
}

// ============================================================================
// Public API for Configuration Access
// ============================================================================

// GetLayer 获取 Layer 配置
func (c *XDSClient) GetLayer(layerID string) (*configv1.Layer, bool) {
	c.cache.mu.RLock()
	defer c.cache.mu.RUnlock()
	layer, exists := c.cache.layers[layerID]
	return layer, exists
}

// GetExperiment 获取 Experiment 配置
func (c *XDSClient) GetExperiment(service string, eid int64) (*configv1.Experiment, bool) {
	c.cache.mu.RLock()
	defer c.cache.mu.RUnlock()
	key := fmt.Sprintf("%s-%d", service, eid)
	experiment, exists := c.cache.experiments[key]
	return experiment, exists
}

// ListLayers 列出所有 Layer
func (c *XDSClient) ListLayers() []*configv1.Layer {
	c.cache.mu.RLock()
	defer c.cache.mu.RUnlock()
	
	layers := make([]*configv1.Layer, 0, len(c.cache.layers))
	for _, layer := range c.cache.layers {
		layers = append(layers, layer)
	}
	return layers
}

// ListExperiments 列出所有 Experiment
func (c *XDSClient) ListExperiments() []*configv1.Experiment {
	c.cache.mu.RLock()
	defer c.cache.mu.RUnlock()
	
	experiments := make([]*configv1.Experiment, 0, len(c.cache.experiments))
	for _, experiment := range c.cache.experiments {
		experiments = append(experiments, experiment)
	}
	return experiments
}

// ============================================================================
// Example Usage
// ============================================================================

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	// 创建节点信息（Envoy 风格）
	node := &configv1.Node{
		Id:      "experiment-dataplane-001",
		Cluster: "production",
		Locality: &configv1.Locality{
			Region: "us-west",
			Zone:   "us-west-1a",
		},
		UserAgentName:    "experiment-dataplane",
		UserAgentVersion: "1.0.0",
		Metadata: &structpb.Struct{
			Fields: map[string]*structpb.Value{
				"environment": structpb.NewStringValue("production"),
				"datacenter":  structpb.NewStringValue("dc1"),
			},
		},
	}

	client, err := NewXDSClient("localhost:50052", node)
	if err != nil {
		log.Fatalf("Failed to create xDS client: %v", err)
	}
	defer client.Close()

	// 示例1: SotW xDS 订阅所有 Layer
	log.Println("=== Testing State of the World xDS ===")
	go func() {
		if err := client.SubscribeLayersSotW(ctx, []string{}); err != nil {
			log.Printf("SotW Layer subscription error: %v", err)
		}
	}()

	// 示例2: Delta xDS 订阅特定 Layer
	log.Println("=== Testing Delta xDS ===")
	go func() {
		layerNames := []string{"payment-layer", "recommendation-layer"}
		if err := client.SubscribeLayersDelta(ctx, layerNames); err != nil {
			log.Printf("Delta Layer subscription error: %v", err)
		}
	}()

	// 示例3: 订阅 Experiment 资源
	go func() {
		experimentNames := []string{"payment-exp-001", "recommendation-exp-002"}
		if err := client.SubscribeExperimentsDelta(ctx, experimentNames); err != nil {
			log.Printf("Delta Experiment subscription error: %v", err)
		}
	}()

	// 等待一段时间，然后查询配置
	time.Sleep(5 * time.Second)

	// 查询配置示例
	if layer, exists := client.GetLayer("payment-layer"); exists {
		log.Printf("Found layer: %s (priority=%d)", layer.LayerId, layer.Priority)
	}

	layers := client.ListLayers()
	log.Printf("Total layers: %d", len(layers))

	experiments := client.ListExperiments()
	log.Printf("Total experiments: %d", len(experiments))

	// 等待测试完成
	<-ctx.Done()
	log.Println("xDS client test completed")
}