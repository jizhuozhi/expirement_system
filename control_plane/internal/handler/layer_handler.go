package handler

import (
	"net/http"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
	"github.com/georgeji/experiment-system/control-plane/internal/state"
	"github.com/gin-gonic/gin"
	"go.uber.org/zap"
)

// LayerHandler Layer API Handler
type LayerHandler struct {
	state  *state.ConfigState
	logger *zap.Logger
}

// NewLayerHandler 创建 Handler
func NewLayerHandler(state *state.ConfigState, logger *zap.Logger) *LayerHandler {
	return &LayerHandler{
		state:  state,
		logger: logger,
	}
}

// CreateLayer 创建 Layer
// POST /api/layers
func (h *LayerHandler) CreateLayer(c *gin.Context) {
	var req models.Layer
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// 写入数据库 + 更新内存 + 推送
	if err := h.state.CreateLayer(c.Request.Context(), &req); err != nil {
		h.logger.Error("create layer failed", zap.Error(err))
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusCreated, req)
}

// UpdateLayer 更新 Layer
// PUT /api/layers/:layer_id
func (h *LayerHandler) UpdateLayer(c *gin.Context) {
	layerID := c.Param("layer_id")

	var req models.Layer
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	req.LayerID = layerID

	if err := h.state.UpdateLayer(c.Request.Context(), &req); err != nil {
		h.logger.Error("update layer failed", zap.Error(err))
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, req)
}

// DeleteLayer 删除 Layer
// DELETE /api/layers/:layer_id
func (h *LayerHandler) DeleteLayer(c *gin.Context) {
	layerID := c.Param("layer_id")

	if err := h.state.DeleteLayer(c.Request.Context(), layerID); err != nil {
		h.logger.Error("delete layer failed", zap.Error(err))
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"message": "deleted"})
}

// GetLayer 获取 Layer
// GET /api/layers/:layer_id
func (h *LayerHandler) GetLayer(c *gin.Context) {
	layerID := c.Param("layer_id")

	// 从内存读取（零拷贝）
	layer, ok := h.state.GetLayer(layerID)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "layer not found"})
		return
	}

	c.JSON(http.StatusOK, layer)
}

// ListLayers 列出 Layers
// GET /api/layers?service=xxx
func (h *LayerHandler) ListLayers(c *gin.Context) {
	service := c.Query("service")

	// 从内存读取
	layers := h.state.ListLayers(service)

	c.JSON(http.StatusOK, gin.H{
		"layers": layers,
		"total":  len(layers),
	})
}

// RegisterRoutes 注册路由
func (h *LayerHandler) RegisterRoutes(r *gin.RouterGroup) {
	r.POST("/layers", h.CreateLayer)
	r.GET("/layers", h.ListLayers)
	r.GET("/layers/:layer_id", h.GetLayer)
	r.PUT("/layers/:layer_id", h.UpdateLayer)
	r.DELETE("/layers/:layer_id", h.DeleteLayer)
}
