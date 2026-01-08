package repository

import (
	"context"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
)

// Repository 数据访问接口
type Repository interface {
	// Layer
	CreateLayer(ctx context.Context, layer *models.Layer) error
	UpdateLayer(ctx context.Context, layer *models.Layer) error
	DeleteLayer(ctx context.Context, layerID string) error
	GetLayer(ctx context.Context, layerID string) (*models.Layer, error)
	ListLayers(ctx context.Context, params ListLayersParams) ([]*models.Layer, error)

	// Experiment
	CreateExperiment(ctx context.Context, exp *models.Experiment) error
	UpdateExperiment(ctx context.Context, exp *models.Experiment) error
	DeleteExperiment(ctx context.Context, eid int32) error
	GetExperiment(ctx context.Context, eid int32) (*models.Experiment, error)
	ListExperiments(ctx context.Context, params ListExperimentsParams) ([]*models.Experiment, error)
}

// ListLayersParams 查询参数
type ListLayersParams struct {
	Service string
	Enabled *bool
}

// ListExperimentsParams 查询参数
type ListExperimentsParams struct {
	Service string
	Status  string
}
