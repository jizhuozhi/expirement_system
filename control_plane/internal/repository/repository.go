package repository

// Repository combines all data access interfaces
type Repository interface {
	LayerRepository
	ExperimentRepository
	ChangeLogRepository
}
