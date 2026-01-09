package repository

import (
	"context"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
)

// ExperimentRepository handles experiment-related operations
type ExperimentRepository interface {
	CreateExperiment(ctx context.Context, exp *models.Experiment) error
	UpdateExperiment(ctx context.Context, exp *models.Experiment) error
	DeleteExperiment(ctx context.Context, eid int32) error
	GetExperiment(ctx context.Context, eid int32) (*models.Experiment, error)
	ListExperiments(ctx context.Context, params ListExperimentsParams) ([]*models.Experiment, error)
}

// ListExperimentsParams query parameters for listing experiments
type ListExperimentsParams struct {
	Service string
	Status  string
}