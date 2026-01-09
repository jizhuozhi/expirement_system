package repository

import (
	"context"
	"database/sql"
	"fmt"
	"strconv"
	"time"

	"github.com/georgeji/experiment-system/control-plane/internal/models"
)

// Database operations
type PostgresRepo struct {
	db *sql.DB
}

// Database operations
func NewPostgresRepo(db *sql.DB) Repository {
	return &PostgresRepo{db: db}
}

// Database operations
// Database operations
// Database operations

func (r *PostgresRepo) CreateLayer(ctx context.Context, layer *models.Layer) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		// Database operations
		if err := r.createLayerInTx(ctx, tx, layer); err != nil {
			return fmt.Errorf("create layer: %w", err)
		}
		
		// Database operations
		if err := r.writeChangeLogInTx(ctx, tx, "layer", layer.LayerID, "create"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) UpdateLayer(ctx context.Context, layer *models.Layer) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		if err := r.updateLayerInTx(ctx, tx, layer); err != nil {
			return fmt.Errorf("update layer: %w", err)
		}
		
		if err := r.writeChangeLogInTx(ctx, tx, "layer", layer.LayerID, "update"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) DeleteLayer(ctx context.Context, layerID string) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		if err := r.deleteLayerInTx(ctx, tx, layerID); err != nil {
			return fmt.Errorf("delete layer: %w", err)
		}
		
		if err := r.writeChangeLogInTx(ctx, tx, "layer", layerID, "delete"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) GetLayer(ctx context.Context, layerID string) (*models.Layer, error) {
	query := `
		SELECT layer_id, version, priority, hash_key, salt, enabled, ranges, services, metadata, created_by, created_at, updated_at
		FROM layers WHERE layer_id = $1`
	
	layer := &models.Layer{}
	
	err := r.db.QueryRowContext(ctx, query, layerID).Scan(
		&layer.LayerID, &layer.Version, &layer.Priority, &layer.HashKey, &layer.Salt, &layer.Enabled,
		&layer.Ranges, &layer.Services, &layer.Metadata, &layer.CreatedBy, &layer.CreatedAt, &layer.UpdatedAt)
	
	return layer, err
}

func (r *PostgresRepo) ListLayers(ctx context.Context, params ListLayersParams) ([]*models.Layer, error) {
	query := `
		SELECT layer_id, version, priority, hash_key, salt, enabled, ranges, services, metadata, created_by, created_at, updated_at
		FROM layers WHERE 1=1`
	args := []interface{}{}
	argIndex := 1
	
	if params.Service != "" {
		query += fmt.Sprintf(" AND $%d = ANY(services)", argIndex)
		args = append(args, params.Service)
		argIndex++
	}
	
	if params.Enabled != nil {
		query += fmt.Sprintf(" AND enabled = $%d", argIndex)
		args = append(args, *params.Enabled)
		argIndex++
	}
	
	query += " ORDER BY priority ASC, layer_id ASC"
	
	rows, err := r.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var layers []*models.Layer
	for rows.Next() {
		layer := &models.Layer{}
		
		err := rows.Scan(&layer.LayerID, &layer.Version, &layer.Priority, &layer.HashKey, &layer.Salt, &layer.Enabled,
			&layer.Ranges, &layer.Services, &layer.Metadata, &layer.CreatedBy, &layer.CreatedAt, &layer.UpdatedAt)
		if err != nil {
			return nil, err
		}
		
		layers = append(layers, layer)
	}
	
	return layers, rows.Err()
}

// Database operations
// Database operations
// Database operations

func (r *PostgresRepo) CreateExperiment(ctx context.Context, exp *models.Experiment) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		if err := r.createExperimentInTx(ctx, tx, exp); err != nil {
			return fmt.Errorf("create experiment: %w", err)
		}
		
		if err := r.writeChangeLogInTx(ctx, tx, "experiment", strconv.Itoa(int(exp.EID)), "create"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) UpdateExperiment(ctx context.Context, exp *models.Experiment) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		if err := r.updateExperimentInTx(ctx, tx, exp); err != nil {
			return fmt.Errorf("update experiment: %w", err)
		}
		
		if err := r.writeChangeLogInTx(ctx, tx, "experiment", strconv.Itoa(int(exp.EID)), "update"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) DeleteExperiment(ctx context.Context, eid int32) error {
	return r.withTransaction(ctx, func(tx *sql.Tx) error {
		if err := r.deleteExperimentInTx(ctx, tx, eid); err != nil {
			return fmt.Errorf("delete experiment: %w", err)
		}
		
		if err := r.writeChangeLogInTx(ctx, tx, "experiment", strconv.Itoa(int(eid)), "delete"); err != nil {
			return fmt.Errorf("write change log: %w", err)
		}
		
		return nil
	})
}

func (r *PostgresRepo) GetExperiment(ctx context.Context, eid int32) (*models.Experiment, error) {
	query := `
		SELECT eid, service, name, rule, variants, metadata, status, created_by, created_at, updated_at
		FROM experiments WHERE eid = $1`
	
	exp := &models.Experiment{}
	
	err := r.db.QueryRowContext(ctx, query, eid).Scan(
		&exp.EID, &exp.Service, &exp.Name, &exp.Rule, &exp.Variants, &exp.Metadata, &exp.Status, &exp.CreatedBy, &exp.CreatedAt, &exp.UpdatedAt)
	
	return exp, err
}

func (r *PostgresRepo) ListExperiments(ctx context.Context, params ListExperimentsParams) ([]*models.Experiment, error) {
	query := `
		SELECT eid, service, name, rule, variants, metadata, status, created_by, created_at, updated_at
		FROM experiments WHERE 1=1`
	args := []interface{}{}
	argIndex := 1
	
	if params.Service != "" {
		query += fmt.Sprintf(" AND service = $%d", argIndex)
		args = append(args, params.Service)
		argIndex++
	}
	
	if params.Status != "" {
		query += fmt.Sprintf(" AND status = $%d", argIndex)
		args = append(args, params.Status)
		argIndex++
	}
	
	query += " ORDER BY eid ASC"
	
	rows, err := r.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var experiments []*models.Experiment
	for rows.Next() {
		exp := &models.Experiment{}
		
		err := rows.Scan(&exp.EID, &exp.Service, &exp.Name, &exp.Rule, &exp.Variants, &exp.Metadata, &exp.Status, &exp.CreatedBy, &exp.CreatedAt, &exp.UpdatedAt)
		if err != nil {
			return nil, err
		}
		
		experiments = append(experiments, exp)
	}
	
	return experiments, rows.Err()
}

// Database operations
// 流水表轮询
// Database operations

func (r *PostgresRepo) GetChangeLogAfter(ctx context.Context, afterID int64, limit int) ([]*ChangeLogEntry, error) {
	query := `
		SELECT id, entity_type, entity_id, operation, created_at
		FROM config_change_log
		WHERE id > $1
		ORDER BY id ASC
		LIMIT $2`
	
	rows, err := r.db.QueryContext(ctx, query, afterID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var entries []*ChangeLogEntry
	for rows.Next() {
		entry := &ChangeLogEntry{}
		err := rows.Scan(&entry.ID, &entry.EntityType, &entry.EntityID, &entry.Operation, &entry.CreatedAt)
		if err != nil {
			return nil, err
		}
		entries = append(entries, entry)
	}
	
	return entries, rows.Err()
}

func (r *PostgresRepo) GetLatestChangeLogID(ctx context.Context) (int64, error) {
	var id sql.NullInt64
	query := `SELECT MAX(id) FROM config_change_log`
	
	err := r.db.QueryRowContext(ctx, query).Scan(&id)
	if err != nil {
		return 0, err
	}
	
	if !id.Valid {
		return 0, nil // 表为空
	}
	
	return id.Int64, nil
}

// Database operations
// 私有辅助方法
// Database operations

// Database operations
func (r *PostgresRepo) withTransaction(ctx context.Context, fn func(tx *sql.Tx) error) error {
	tx, err := r.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin tx: %w", err)
	}
	
	defer func() {
		if p := recover(); p != nil {
			tx.Rollback()
			panic(p)
		} else if err != nil {
			tx.Rollback()
		} else {
			err = tx.Commit()
		}
	}()
	
	err = fn(tx)
	return err
}

// Database operations
func (r *PostgresRepo) writeChangeLogInTx(ctx context.Context, tx *sql.Tx, entityType, entityID, operation string) error {
	query := `
		INSERT INTO config_change_log (entity_type, entity_id, operation)
		VALUES ($1, $2, $3)`
	
	_, err := tx.ExecContext(ctx, query, entityType, entityID, operation)
	return err
}

// Database operations
func (r *PostgresRepo) createLayerInTx(ctx context.Context, tx *sql.Tx, layer *models.Layer) error {
	query := `
		INSERT INTO layers (layer_id, version, priority, hash_key, salt, enabled, ranges, services, metadata, created_by, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`
	
	now := time.Now()
	_, err := tx.ExecContext(ctx, query,
		layer.LayerID, layer.Version, layer.Priority, layer.HashKey, layer.Salt, layer.Enabled,
		layer.Ranges, layer.Services, layer.Metadata, layer.CreatedBy, now, now)
	
	return err
}

// Database operations
func (r *PostgresRepo) updateLayerInTx(ctx context.Context, tx *sql.Tx, layer *models.Layer) error {
	query := `
		UPDATE layers 
		SET version = $2, priority = $3, hash_key = $4, salt = $5, enabled = $6, ranges = $7, services = $8, metadata = $9, updated_at = $10
		WHERE layer_id = $1`
	
	_, err := tx.ExecContext(ctx, query,
		layer.LayerID, layer.Version, layer.Priority, layer.HashKey, layer.Salt, layer.Enabled,
		layer.Ranges, layer.Services, layer.Metadata, time.Now())
	
	return err
}

// Database operations
func (r *PostgresRepo) deleteLayerInTx(ctx context.Context, tx *sql.Tx, layerID string) error {
	query := `DELETE FROM layers WHERE layer_id = $1`
	_, err := tx.ExecContext(ctx, query, layerID)
	return err
}

// Database operations
func (r *PostgresRepo) createExperimentInTx(ctx context.Context, tx *sql.Tx, exp *models.Experiment) error {
	query := `
		INSERT INTO experiments (eid, service, name, rule, variants, metadata, status, created_by, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)`
	
	now := time.Now()
	_, err := tx.ExecContext(ctx, query,
		exp.EID, exp.Service, exp.Name, exp.Rule, exp.Variants, exp.Metadata, exp.Status, exp.CreatedBy, now, now)
	
	return err
}

// Database operations
func (r *PostgresRepo) updateExperimentInTx(ctx context.Context, tx *sql.Tx, exp *models.Experiment) error {
	query := `
		UPDATE experiments 
		SET service = $2, name = $3, rule = $4, variants = $5, metadata = $6, status = $7, updated_at = $8
		WHERE eid = $1`
	
	_, err := tx.ExecContext(ctx, query,
		exp.EID, exp.Service, exp.Name, exp.Rule, exp.Variants, exp.Metadata, exp.Status, time.Now())
	
	return err
}

// Database operations
func (r *PostgresRepo) deleteExperimentInTx(ctx context.Context, tx *sql.Tx, eid int32) error {
	query := `DELETE FROM experiments WHERE eid = $1`
	_, err := tx.ExecContext(ctx, query, eid)
	return err
}