package repository

import (
	"context"
	"time"
)

// ChangeLogEntry represents a changelog record
type ChangeLogEntry struct {
	ID         int64     `db:"id"`
	EntityType string    `db:"entity_type"`
	EntityID   string    `db:"entity_id"`
	Operation  string    `db:"operation"`
	CreatedAt  time.Time `db:"created_at"`
}

// ChangeLogRepository handles changelog-related operations
type ChangeLogRepository interface {
	GetChangeLogAfter(ctx context.Context, afterID int64, limit int) ([]*ChangeLogEntry, error)
	GetLatestChangeLogID(ctx context.Context) (int64, error)
	WriteChangeLog(ctx context.Context, entityType, entityID, operation string) error
}