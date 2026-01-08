package sync

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"go.uber.org/zap"
)

// ChangeLogPoller 流水表轮询器
type ChangeLogPoller struct {
	db       *pgxpool.Pool
	logger   *zap.Logger
	interval time.Duration
	handler  ChangeHandler

	lastID   int64 // 上次轮询的最大 ID
	stopCh   chan struct{}
	stoppedCh chan struct{}
}

// ChangeHandler 变更处理函数
type ChangeHandler func(change *ChangeLogEntry) error

// ChangeLogEntry 流水表记录（极简版：只有 ID）
type ChangeLogEntry struct {
	ID         int64     `db:"id"`
	EntityType string    `db:"entity_type"` // "layer" 或 "experiment"
	EntityID   string    `db:"entity_id"`   // layer_id 或 eid
	Operation  string    `db:"operation"`   // "create", "update", "delete"
	CreatedAt  time.Time `db:"created_at"`
}

// NewChangeLogPoller 创建轮询器
func NewChangeLogPoller(
	db *pgxpool.Pool,
	interval time.Duration,
	handler ChangeHandler,
	logger *zap.Logger,
) *ChangeLogPoller {
	return &ChangeLogPoller{
		db:        db,
		logger:    logger,
		interval:  interval,
		handler:   handler,
		lastID:    0,
		stopCh:    make(chan struct{}),
		stoppedCh: make(chan struct{}),
	}
}

// Start 启动轮询
func (p *ChangeLogPoller) Start(ctx context.Context) error {
	defer close(p.stoppedCh)

	// 初始化：获取当前最大 ID
	if err := p.initLastID(ctx); err != nil {
		return fmt.Errorf("init last id: %w", err)
	}

	p.logger.Info("change log poller started",
		zap.Int64("last_id", p.lastID),
		zap.Duration("interval", p.interval),
	)

	ticker := time.NewTicker(p.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			p.logger.Info("change log poller stopped by context")
			return ctx.Err()
		case <-p.stopCh:
			p.logger.Info("change log poller stopped")
			return nil
		case <-ticker.C:
			if err := p.poll(ctx); err != nil {
				p.logger.Error("poll changes failed", zap.Error(err))
				// 继续轮询，不退出
			}
		}
	}
}

// Stop 停止轮询
func (p *ChangeLogPoller) Stop() {
	close(p.stopCh)
	<-p.stoppedCh
}

// initLastID 初始化 lastID（获取当前最大值）
func (p *ChangeLogPoller) initLastID(ctx context.Context) error {
	query := `SELECT COALESCE(MAX(id), 0) FROM config_change_log`
	
	var maxID int64
	err := p.db.QueryRow(ctx, query).Scan(&maxID)
	if err != nil {
		return err
	}

	p.lastID = maxID
	p.logger.Info("initialized last_id", zap.Int64("last_id", p.lastID))
	return nil
}

// poll 轮询新变更
func (p *ChangeLogPoller) poll(ctx context.Context) error {
	// 查询 ID > lastID 的新记录（只查 4 个字段）
	query := `
		SELECT id, entity_type, entity_id, operation, created_at
		FROM config_change_log
		WHERE id > $1
		ORDER BY id ASC
		LIMIT 1000
	`

	rows, err := p.db.Query(ctx, query, p.lastID)
	if err != nil {
		return fmt.Errorf("query changes: %w", err)
	}
	defer rows.Close()

	count := 0
	var maxID int64 = p.lastID

	for rows.Next() {
		var entry ChangeLogEntry

		err := rows.Scan(
			&entry.ID,
			&entry.EntityType,
			&entry.EntityID,
			&entry.Operation,
			&entry.CreatedAt,
		)
		if err != nil {
			p.logger.Error("scan row failed", zap.Error(err))
			continue
		}

		// 调用处理函数
		if err := p.handler(&entry); err != nil {
			p.logger.Error("handle change failed",
				zap.Int64("id", entry.ID),
				zap.String("entity_type", entry.EntityType),
				zap.String("operation", entry.Operation),
				zap.Error(err),
			)
			// 继续处理下一条，不中断
		}

		if entry.ID > maxID {
			maxID = entry.ID
		}
		count++
	}

	if err := rows.Err(); err != nil {
		return fmt.Errorf("rows iteration: %w", err)
	}

	// 更新 lastID
	if maxID > p.lastID {
		p.lastID = maxID
		p.logger.Debug("polled changes",
			zap.Int("count", count),
			zap.Int64("new_last_id", p.lastID),
		)
	}

	return nil
}

// GetLastID 获取当前 lastID（用于监控）
func (p *ChangeLogPoller) GetLastID() int64 {
	return p.lastID
}
