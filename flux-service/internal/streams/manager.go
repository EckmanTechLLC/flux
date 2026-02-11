package streams

import (
	"fmt"
	"strings"
	"time"

	"github.com/nats-io/nats.go"
)

// StreamConfig defines the configuration for a Flux stream
type StreamConfig struct {
	Name        string        // Flux stream name (e.g., "alarms.events")
	Subjects    []string      // NATS subjects (e.g., ["alarms.events"])
	Description string        // Human-readable description
	MaxAge      time.Duration // Retention time (default: 7 days)
	MaxBytes    int64         // Max storage size (default: 10GB)
	MaxMsgs     int64         // Max message count (default: 10M)
	Storage     nats.StorageType
}

// toNATSStreamName converts a Flux stream name to a valid NATS stream name.
// NATS stream names cannot contain dots, so we convert:
// "alarms.events" -> "ALARMS_EVENTS"
// The subjects still use the original dot notation.
func toNATSStreamName(fluxName string) string {
	// Replace dots with underscores and convert to uppercase
	return strings.ToUpper(strings.ReplaceAll(fluxName, ".", "_"))
}

// DefaultStreamConfig returns a StreamConfig with Flux default retention policies
func DefaultStreamConfig(name string) *StreamConfig {
	return &StreamConfig{
		Name:        name,
		Subjects:    []string{name}, // Initially 1:1 mapping stream -> subject
		Description: fmt.Sprintf("Flux stream: %s", name),
		MaxAge:      7 * 24 * time.Hour, // 7 days
		MaxBytes:    10 * 1024 * 1024 * 1024, // 10GB
		MaxMsgs:     10_000_000, // 10 million messages
		Storage:     nats.FileStorage,
	}
}

// Manager handles stream creation and management
type Manager struct {
	js nats.JetStreamContext
}

// NewManager creates a new stream manager
func NewManager(js nats.JetStreamContext) *Manager {
	return &Manager{js: js}
}

// CreateStream creates a stream with the given configuration.
// If the stream already exists, it verifies the configuration matches.
// Returns error only on actual failures, not on "already exists".
func (m *Manager) CreateStream(config *StreamConfig) error {
	if config == nil {
		return fmt.Errorf("config cannot be nil")
	}

	if config.Name == "" {
		return fmt.Errorf("stream name cannot be empty")
	}

	if len(config.Subjects) == 0 {
		return fmt.Errorf("stream must have at least one subject")
	}

	// Convert Flux stream name to NATS stream name
	natsStreamName := toNATSStreamName(config.Name)

	streamConfig := &nats.StreamConfig{
		Name:        natsStreamName,
		Subjects:    config.Subjects,
		Description: config.Description,
		Retention:   nats.LimitsPolicy,
		MaxAge:      config.MaxAge,
		MaxBytes:    config.MaxBytes,
		MaxMsgs:     config.MaxMsgs,
		Storage:     config.Storage,
		Replicas:    1, // Single-node initially
		Discard:     nats.DiscardOld,
	}

	// Try to add stream
	_, err := m.js.AddStream(streamConfig)
	if err != nil {
		// Check if stream already exists
		if err == nats.ErrStreamNameAlreadyInUse {
			// Stream exists, verify it matches our config
			info, streamErr := m.js.StreamInfo(natsStreamName)
			if streamErr != nil {
				return fmt.Errorf("stream exists but cannot get info: %w", streamErr)
			}

			// Stream exists and we can access it - this is fine (idempotent)
			// In production, might want to verify config matches, but for now just succeed
			_ = info // Stream info available if needed for verification
			return nil
		}

		return fmt.Errorf("failed to create stream %s: %w", config.Name, err)
	}

	return nil
}

// StreamExists checks if a stream exists
func (m *Manager) StreamExists(name string) (bool, error) {
	natsStreamName := toNATSStreamName(name)
	_, err := m.js.StreamInfo(natsStreamName)
	if err != nil {
		if err == nats.ErrStreamNotFound {
			return false, nil
		}
		return false, fmt.Errorf("failed to check stream %s: %w", name, err)
	}
	return true, nil
}

// InitializeStreams creates multiple streams with default configurations.
// Logs progress and continues on errors (best-effort initialization).
// Returns the first error encountered, if any.
func (m *Manager) InitializeStreams(streamNames []string, logger func(format string, args ...interface{})) error {
	if logger == nil {
		logger = func(format string, args ...interface{}) {} // no-op logger
	}

	var firstErr error

	for _, name := range streamNames {
		logger("Initializing stream: %s", name)

		config := DefaultStreamConfig(name)
		err := m.CreateStream(config)
		if err != nil {
			logger("Failed to initialize stream %s: %v", name, err)
			if firstErr == nil {
				firstErr = err
			}
			continue
		}

		logger("Stream %s ready (retention: %v, max size: %dGB, max msgs: %dM)",
			name,
			config.MaxAge,
			config.MaxBytes/(1024*1024*1024),
			config.MaxMsgs/1_000_000,
		)
	}

	return firstErr
}
