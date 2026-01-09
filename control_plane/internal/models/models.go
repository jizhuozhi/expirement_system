package models

import (
	"database/sql/driver"
	"encoding/json"
	"time"
)

// User account information
type User struct {
	ID           string    `db:"id" json:"id"`
	Email        string    `db:"email" json:"email"`
	PasswordHash string    `db:"password_hash" json:"-"`
	Name         string    `db:"name" json:"name"`
	Role         string    `db:"role" json:"role"` // admin, user, viewer
	CreatedAt    time.Time `db:"created_at" json:"created_at"`
	UpdatedAt    time.Time `db:"updated_at" json:"updated_at"`
}

// Experiment layer configuration
type Layer struct {
	LayerID   string           `db:"layer_id" json:"layer_id"`
	Version   string           `db:"version" json:"version"`
	Priority  int32            `db:"priority" json:"priority"`
	HashKey   string           `db:"hash_key" json:"hash_key"`
	Salt      string           `db:"salt" json:"salt"`
	Enabled   bool             `db:"enabled" json:"enabled"`
	Ranges    JSONBucketRanges `db:"ranges" json:"ranges"`
	Services  JSONStringArray  `db:"services" json:"services"`
	Metadata  JSONMap          `db:"metadata" json:"metadata"`
	CreatedBy string           `db:"created_by" json:"created_by"`
	CreatedAt time.Time        `db:"created_at" json:"created_at"`
	UpdatedAt time.Time        `db:"updated_at" json:"updated_at"`
}

// Bucket range for experiment allocation
type BucketRange struct {
	Start uint32 `json:"start"`
	End   uint32 `json:"end"`
	VID   int32  `json:"vid"`
}

// Experiment definition with rules and variants
type Experiment struct {
	EID       int32        `db:"eid" json:"eid"`
	Service   string       `db:"service" json:"service"`
	Name      string       `db:"name" json:"name"`
	Rule      JSONRuleNode `db:"rule" json:"rule"`
	Variants  JSONVariants `db:"variants" json:"variants"`
	Metadata  JSONMap      `db:"metadata" json:"metadata"`
	Status    string       `db:"status" json:"status"` // active, paused, stopped
	CreatedBy string       `db:"created_by" json:"created_by"`
	CreatedAt time.Time    `db:"created_at" json:"created_at"`
	UpdatedAt time.Time    `db:"updated_at" json:"updated_at"`
}

// Rule evaluation node
type RuleNode struct {
	Type     string     `json:"type"`
	Field    string     `json:"field,omitempty"`
	Op       string     `json:"op,omitempty"`
	Values   []string   `json:"values,omitempty"`
	Children []RuleNode `json:"children,omitempty"`
}

// Experiment variant with parameters
type Variant struct {
	VID    int32                  `json:"vid"`
	Params map[string]interface{} `json:"params"`
}

// Configuration version tracking
type ConfigVersion struct {
	Version   string    `db:"version" json:"version"`
	Timestamp int64     `db:"timestamp" json:"timestamp"`
	ChangeLog string    `db:"change_log" json:"change_log"`
	CreatedBy string    `db:"created_by" json:"created_by"`
	CreatedAt time.Time `db:"created_at" json:"created_at"`
}

// Data plane instance registration
type DataPlaneInstance struct {
	ID             string    `db:"id" json:"id"`
	Hostname       string    `db:"hostname" json:"hostname"`
	IPAddress      string    `db:"ip_address" json:"ip_address"`
	Version        string    `db:"version" json:"version"`
	CurrentVersion string    `db:"current_version" json:"current_version"` // 配置版本
	LastHeartbeat  time.Time `db:"last_heartbeat" json:"last_heartbeat"`
	Status         string    `db:"status" json:"status"` // online, offline, error
	Metadata       JSONMap   `db:"metadata" json:"metadata"`
	CreatedAt      time.Time `db:"created_at" json:"created_at"`
	UpdatedAt      time.Time `db:"updated_at" json:"updated_at"`
}

// JSON serialization helpers

type JSONBucketRanges []BucketRange

func (j JSONBucketRanges) Value() (driver.Value, error) {
	return json.Marshal(j)
}

func (j *JSONBucketRanges) Scan(value interface{}) error {
	if value == nil {
		*j = []BucketRange{}
		return nil
	}
	bytes, ok := value.([]byte)
	if !ok {
		return nil
	}
	return json.Unmarshal(bytes, j)
}

type JSONStringArray []string

func (j JSONStringArray) Value() (driver.Value, error) {
	return json.Marshal(j)
}

func (j *JSONStringArray) Scan(value interface{}) error {
	if value == nil {
		*j = []string{}
		return nil
	}
	bytes, ok := value.([]byte)
	if !ok {
		return nil
	}
	return json.Unmarshal(bytes, j)
}

type JSONRuleNode RuleNode

func (j JSONRuleNode) Value() (driver.Value, error) {
	return json.Marshal(j)
}

func (j *JSONRuleNode) Scan(value interface{}) error {
	if value == nil {
		return nil
	}
	bytes, ok := value.([]byte)
	if !ok {
		return nil
	}
	return json.Unmarshal(bytes, (*RuleNode)(j))
}

type JSONVariants []Variant

func (j JSONVariants) Value() (driver.Value, error) {
	return json.Marshal(j)
}

func (j *JSONVariants) Scan(value interface{}) error {
	if value == nil {
		*j = []Variant{}
		return nil
	}
	bytes, ok := value.([]byte)
	if !ok {
		return nil
	}
	return json.Unmarshal(bytes, j)
}

type JSONMap map[string]string

func (j JSONMap) Value() (driver.Value, error) {
	return json.Marshal(j)
}

func (j *JSONMap) Scan(value interface{}) error {
	if value == nil {
		*j = make(map[string]string)
		return nil
	}
	bytes, ok := value.([]byte)
	if !ok {
		return nil
	}
	return json.Unmarshal(bytes, j)
}
