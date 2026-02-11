package streams

import (
	"testing"
	"time"

	"github.com/nats-io/nats.go"
)

// Test helper: Start NATS server with JetStream for testing
// NOTE: These tests require a running NATS server with JetStream enabled
// Use: docker run -p 4222:4222 nats:latest -js

func getTestJetStream(t *testing.T) (nats.JetStreamContext, *nats.Conn) {
	t.Helper()

	// Try flux-nats first (Docker network), then localhost:4223 (local testing)
	natsURLs := []string{"nats://flux-nats:4222", "nats://localhost:4223"}
	var nc *nats.Conn
	var err error

	for _, url := range natsURLs {
		nc, err = nats.Connect(url, nats.Timeout(2*time.Second))
		if err == nil {
			break
		}
	}

	if err != nil {
		t.Skipf("Cannot connect to NATS for testing: %v (start NATS with: docker compose up -d)", err)
		return nil, nil
	}

	js, err := nc.JetStream()
	if err != nil {
		nc.Close()
		t.Skipf("JetStream not available: %v", err)
		return nil, nil
	}

	return js, nc
}

func cleanupStream(t *testing.T, js nats.JetStreamContext, name string) {
	t.Helper()
	natsStreamName := toNATSStreamName(name)
	_ = js.DeleteStream(natsStreamName) // Ignore errors, stream might not exist
}

func TestDefaultStreamConfig(t *testing.T) {
	config := DefaultStreamConfig("test.stream")

	if config.Name != "test.stream" {
		t.Errorf("Name = %s, want test.stream", config.Name)
	}

	if len(config.Subjects) != 1 || config.Subjects[0] != "test.stream" {
		t.Errorf("Subjects = %v, want [test.stream]", config.Subjects)
	}

	if config.MaxAge != 7*24*time.Hour {
		t.Errorf("MaxAge = %v, want 7 days", config.MaxAge)
	}

	expectedBytes := int64(10 * 1024 * 1024 * 1024) // 10GB
	if config.MaxBytes != expectedBytes {
		t.Errorf("MaxBytes = %d, want %d (10GB)", config.MaxBytes, expectedBytes)
	}

	if config.MaxMsgs != 10_000_000 {
		t.Errorf("MaxMsgs = %d, want 10000000", config.MaxMsgs)
	}

	if config.Storage != nats.FileStorage {
		t.Errorf("Storage = %v, want FileStorage", config.Storage)
	}
}

func TestNewManager(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	manager := NewManager(js)
	if manager == nil {
		t.Fatal("NewManager returned nil")
	}

	if manager.js == nil {
		t.Error("Manager.js is nil")
	}
}

func TestCreateStream_Success(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streamName := "test.create.success"
	cleanupStream(t, js, streamName)
	defer cleanupStream(t, js, streamName)

	manager := NewManager(js)
	config := DefaultStreamConfig(streamName)

	err := manager.CreateStream(config)
	if err != nil {
		t.Fatalf("CreateStream failed: %v", err)
	}

	// Verify stream was created (using NATS stream name)
	natsStreamName := toNATSStreamName(streamName)
	info, err := js.StreamInfo(natsStreamName)
	if err != nil {
		t.Fatalf("StreamInfo failed: %v", err)
	}

	if info.Config.Name != natsStreamName {
		t.Errorf("Stream name = %s, want %s", info.Config.Name, natsStreamName)
	}

	if info.Config.Storage != nats.FileStorage {
		t.Errorf("Storage = %v, want FileStorage", info.Config.Storage)
	}
}

func TestCreateStream_CustomConfig(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streamName := "test.custom.config"
	cleanupStream(t, js, streamName)
	defer cleanupStream(t, js, streamName)

	manager := NewManager(js)
	config := &StreamConfig{
		Name:        streamName,
		Subjects:    []string{streamName},
		Description: "Custom test stream",
		MaxAge:      24 * time.Hour, // 1 day
		MaxBytes:    1024 * 1024 * 1024, // 1GB
		MaxMsgs:     1_000_000, // 1M messages
		Storage:     nats.FileStorage,
	}

	err := manager.CreateStream(config)
	if err != nil {
		t.Fatalf("CreateStream failed: %v", err)
	}

	// Verify custom config was applied (using NATS stream name)
	natsStreamName := toNATSStreamName(streamName)
	info, err := js.StreamInfo(natsStreamName)
	if err != nil {
		t.Fatalf("StreamInfo failed: %v", err)
	}

	if info.Config.MaxAge != 24*time.Hour {
		t.Errorf("MaxAge = %v, want 24h", info.Config.MaxAge)
	}

	if info.Config.MaxBytes != 1024*1024*1024 {
		t.Errorf("MaxBytes = %d, want 1GB", info.Config.MaxBytes)
	}

	if info.Config.MaxMsgs != 1_000_000 {
		t.Errorf("MaxMsgs = %d, want 1000000", info.Config.MaxMsgs)
	}
}

func TestCreateStream_Idempotent(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streamName := "test.idempotent"
	cleanupStream(t, js, streamName)
	defer cleanupStream(t, js, streamName)

	manager := NewManager(js)
	config := DefaultStreamConfig(streamName)

	// Create stream first time
	err := manager.CreateStream(config)
	if err != nil {
		t.Fatalf("First CreateStream failed: %v", err)
	}

	// Create same stream again - should not error
	err = manager.CreateStream(config)
	if err != nil {
		t.Errorf("Second CreateStream failed (not idempotent): %v", err)
	}

	// Verify stream still exists and is valid (using NATS stream name)
	natsStreamName := toNATSStreamName(streamName)
	info, err := js.StreamInfo(natsStreamName)
	if err != nil {
		t.Fatalf("StreamInfo failed: %v", err)
	}

	if info.Config.Name != natsStreamName {
		t.Errorf("Stream name = %s, want %s", info.Config.Name, natsStreamName)
	}
}

func TestCreateStream_InvalidConfig(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	manager := NewManager(js)

	tests := []struct {
		name      string
		config    *StreamConfig
		wantError string
	}{
		{
			name:      "nil config",
			config:    nil,
			wantError: "config cannot be nil",
		},
		{
			name: "empty stream name",
			config: &StreamConfig{
				Name:     "",
				Subjects: []string{"test"},
				Storage:  nats.FileStorage,
			},
			wantError: "stream name cannot be empty",
		},
		{
			name: "no subjects",
			config: &StreamConfig{
				Name:     "test.nosubjects",
				Subjects: []string{},
				Storage:  nats.FileStorage,
			},
			wantError: "at least one subject",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := manager.CreateStream(tt.config)
			if err == nil {
				t.Errorf("CreateStream succeeded, want error containing %q", tt.wantError)
				return
			}

			// Check error message contains expected text
			if tt.wantError != "" {
				errMsg := err.Error()
				if !contains(errMsg, tt.wantError) {
					t.Errorf("Error = %q, want error containing %q", errMsg, tt.wantError)
				}
			}
		})
	}
}

func TestStreamExists(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streamName := "test.exists"
	cleanupStream(t, js, streamName)
	defer cleanupStream(t, js, streamName)

	manager := NewManager(js)

	// Stream doesn't exist yet
	exists, err := manager.StreamExists(streamName)
	if err != nil {
		t.Fatalf("StreamExists failed: %v", err)
	}
	if exists {
		t.Error("StreamExists = true, want false (stream not created yet)")
	}

	// Create stream
	config := DefaultStreamConfig(streamName)
	err = manager.CreateStream(config)
	if err != nil {
		t.Fatalf("CreateStream failed: %v", err)
	}

	// Now stream should exist
	exists, err = manager.StreamExists(streamName)
	if err != nil {
		t.Fatalf("StreamExists failed after creation: %v", err)
	}
	if !exists {
		t.Error("StreamExists = false, want true (stream was created)")
	}
}

func TestInitializeStreams(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streams := []string{"test.init.one", "test.init.two", "test.init.three"}
	for _, s := range streams {
		cleanupStream(t, js, s)
		defer cleanupStream(t, js, s)
	}

	manager := NewManager(js)

	// Capture log output
	var logMessages []string
	logger := func(format string, args ...interface{}) {
		logMessages = append(logMessages, format)
	}

	err := manager.InitializeStreams(streams, logger)
	if err != nil {
		t.Errorf("InitializeStreams returned error: %v", err)
	}

	// Verify all streams were created
	for _, streamName := range streams {
		exists, err := manager.StreamExists(streamName)
		if err != nil {
			t.Errorf("StreamExists(%s) failed: %v", streamName, err)
		}
		if !exists {
			t.Errorf("Stream %s was not created", streamName)
		}
	}

	// Verify logging occurred
	if len(logMessages) == 0 {
		t.Error("No log messages captured, expected initialization logs")
	}
}

func TestInitializeStreams_NilLogger(t *testing.T) {
	js, nc := getTestJetStream(t)
	if js == nil {
		return
	}
	defer nc.Close()

	streamName := "test.init.nillogger"
	cleanupStream(t, js, streamName)
	defer cleanupStream(t, js, streamName)

	manager := NewManager(js)

	// Should not panic with nil logger
	err := manager.InitializeStreams([]string{streamName}, nil)
	if err != nil {
		t.Errorf("InitializeStreams with nil logger failed: %v", err)
	}

	// Verify stream was created
	exists, err := manager.StreamExists(streamName)
	if err != nil {
		t.Fatalf("StreamExists failed: %v", err)
	}
	if !exists {
		t.Error("Stream was not created with nil logger")
	}
}

// Helper function
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(substr) == 0 ||
		(len(s) > 0 && len(substr) > 0 && indexOfSubstring(s, substr) >= 0))
}

func indexOfSubstring(s, substr string) int {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return i
		}
	}
	return -1
}
