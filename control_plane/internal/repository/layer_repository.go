package repository

import (
	"context"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
)

// LayerRepository handles layer-related operations
type LayerRepository interface {
	CreateLayer(ctx context.Context, layer *models.Layer) error
	UpdateLayer(ctx context.Context, layer *models.Layer) error
	DeleteLayer(ctx context.Context, layerID string) error
	GetLayer(ctx context.Context, layerID string) (*models.Layer, error)
	ListLayers(ctx context.Context, params ListLayersParams) ([]*models.Layer, error)
}

// ListLayersParams query parameters for listing layers
type ListLayersParams struct {
	Service string
	Enabled *bool
}