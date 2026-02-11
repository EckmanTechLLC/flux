package publisher

import (
	"encoding/json"
	"os"
	"testing"
	"time"

	"github.com/flux/flux-service/internal/model"
	"github.com/flux/flux-service/internal/streams"
	"github.com/nats-io/nats.go"
)

// getTestJetStream connects to NATS for testing
func getTestJetStream(t *testing.T) nats.JetStreamContext {
	t.Helper()

	// Try Docker network first, then localhost
	urls := []string{
		"nats://flux-nats:4222",  // Docker Compose network
		"nats://localhost:4223",  // Local development
	}

	var nc *nats.Conn
	var err error

	for _, url := range urls {
		nc, err = nats.Connect(url, nats.Timeout(2*time.Second))
		if err == nil {
			break
		}
	}

	if err != nil {
		t.Skipf("NATS not available (tried %v): %v", urls, err)
		return nil
	}

	t.Cleanup(func() {
		nc.Close()
	})

	js, err := nc.JetStream()
	if err != nil {
		t.Fatalf("Failed to get JetStream context: %v", err)
	}

	return js
}

// cleanupStream deletes a stream after tests
func cleanupStream(t *testing.T, js nats.JetStreamContext, streamName string) {
	t.Helper()
	// Convert Flux stream name to NATS stream name
	natsStreamName := toNATSStreamName(streamName)
	_ = js.DeleteStream(natsStreamName)
}

// toNATSStreamName converts Flux stream name to NATS stream name
func toNATSStreamName(fluxName string) string {
	// Same logic as in streams package
	return natsStreamNameFromFlux(fluxName)
}

// Helper to convert stream names (duplicated from streams package for testing)
func natsStreamNameFromFlux(fluxName string) string {
	// This matches the logic in streams.toNATSStreamName
	result := ""
	for _, char := range fluxName {
		if char == '.' {
			result += "_"
		} else {
			if char >= 'a' && char <= 'z' {
				result += string(char - 'a' + 'A')
			} else if char >= 'A' && char <= 'Z' {
				result += string(char)
			} else {
				result += string(char)
			}
		}
	}
	return result
}

func TestNewPublisher(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)

	pub := NewPublisher(js, streamManager)

	if pub == nil {
		t.Fatal("NewPublisher returned nil")
	}
	if pub.js == nil {
		t.Error("Publisher JetStream context is nil")
	}
	if pub.streamManager == nil {
		t.Error("Publisher stream manager is nil")
	}
}

func TestPublish_ValidEvent(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	streamName := "test.publish.valid"
	t.Cleanup(func() { cleanupStream(t, js, streamName) })

	// Create stream first
	config := streams.DefaultStreamConfig(streamName)
	if err := streamManager.CreateStream(config); err != nil {
		t.Fatalf("Failed to create test stream: %v", err)
	}

	// Create valid event
	event := &model.Event{
		Stream:    streamName,
		Source:    "test-producer",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test": "data", "value": 42}`),
		Key:       "test-key",
		Schema:    "test.v1",
	}

	// Publish event
	result, err := pub.Publish(event)
	if err != nil {
		t.Fatalf("Publish failed: %v", err)
	}

	// Verify result
	if result == nil {
		t.Fatal("Publish result is nil")
	}
	if result.EventID == "" {
		t.Error("EventID is empty")
	}
	if result.Stream != streamName {
		t.Errorf("Stream = %q, want %q", result.Stream, streamName)
	}
	if result.Sequence == 0 {
		t.Error("Sequence is 0 (should be positive)")
	}

	// Verify eventId was generated
	if event.EventID == "" {
		t.Error("Event EventID not generated")
	}
	if result.EventID != event.EventID {
		t.Errorf("Result EventID %q != Event EventID %q", result.EventID, event.EventID)
	}
}

func TestPublish_GeneratesEventID(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	streamName := "test.publish.eventid"
	t.Cleanup(func() { cleanupStream(t, js, streamName) })

	// Create stream
	config := streams.DefaultStreamConfig(streamName)
	if err := streamManager.CreateStream(config); err != nil {
		t.Fatalf("Failed to create test stream: %v", err)
	}

	// Event without eventId
	event := &model.Event{
		Stream:    streamName,
		Source:    "test-producer",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test": "data"}`),
	}

	// Verify eventId is empty before publish
	if event.EventID != "" {
		t.Error("EventID should be empty before publish")
	}

	// Publish
	result, err := pub.Publish(event)
	if err != nil {
		t.Fatalf("Publish failed: %v", err)
	}

	// Verify eventId was generated
	if event.EventID == "" {
		t.Error("EventID was not generated")
	}
	if result.EventID == "" {
		t.Error("Result EventID is empty")
	}
	if result.EventID != event.EventID {
		t.Error("Result EventID does not match event EventID")
	}
}

func TestPublish_AutoCreateStream(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	streamName := "test.publish.autocreate"
	t.Cleanup(func() { cleanupStream(t, js, streamName) })

	// Do NOT create stream beforehand
	// Verify stream doesn't exist
	exists, err := streamManager.StreamExists(streamName)
	if err != nil {
		t.Fatalf("Failed to check stream: %v", err)
	}
	if exists {
		t.Fatal("Stream should not exist before test")
	}

	// Publish event to non-existent stream
	event := &model.Event{
		Stream:    streamName,
		Source:    "test-producer",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test": "autocreate"}`),
	}

	result, err := pub.Publish(event)
	if err != nil {
		t.Fatalf("Publish to non-existent stream failed: %v", err)
	}

	// Verify publish succeeded
	if result == nil {
		t.Fatal("Publish result is nil")
	}
	if result.Sequence == 0 {
		t.Error("Sequence should be positive")
	}

	// Verify stream was auto-created
	exists, err = streamManager.StreamExists(streamName)
	if err != nil {
		t.Fatalf("Failed to check stream after publish: %v", err)
	}
	if !exists {
		t.Error("Stream should exist after auto-create")
	}
}

func TestPublish_InvalidEvent(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	tests := []struct {
		name  string
		event *model.Event
	}{
		{
			name:  "nil event",
			event: nil,
		},
		{
			name: "missing stream",
			event: &model.Event{
				Source:    "test",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{}`),
			},
		},
		{
			name: "missing source",
			event: &model.Event{
				Stream:    "test.stream",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{}`),
			},
		},
		{
			name: "missing timestamp",
			event: &model.Event{
				Stream:  "test.stream",
				Source:  "test",
				Payload: json.RawMessage(`{}`),
			},
		},
		{
			name: "missing payload",
			event: &model.Event{
				Stream:    "test.stream",
				Source:    "test",
				Timestamp: time.Now().UnixMilli(),
			},
		},
		{
			name: "invalid stream name",
			event: &model.Event{
				Stream:    "Invalid-Stream",
				Source:    "test",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{}`),
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := pub.Publish(tt.event)
			if err == nil {
				t.Errorf("Publish should have failed for %s", tt.name)
			}
			if result != nil {
				t.Errorf("Result should be nil on error, got %+v", result)
			}
		})
	}
}

func TestPublish_EventPersistedInNATS(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	streamName := "test.publish.persisted"
	t.Cleanup(func() { cleanupStream(t, js, streamName) })

	// Create stream
	config := streams.DefaultStreamConfig(streamName)
	if err := streamManager.CreateStream(config); err != nil {
		t.Fatalf("Failed to create test stream: %v", err)
	}

	// Publish event
	originalPayload := json.RawMessage(`{"sensor": "temp-01", "value": 23.5, "unit": "celsius"}`)
	event := &model.Event{
		Stream:    streamName,
		Source:    "sensor-gateway",
		Timestamp: time.Now().UnixMilli(),
		Payload:   originalPayload,
		Key:       "temp-01",
		Schema:    "sensor.reading.v1",
	}

	result, err := pub.Publish(event)
	if err != nil {
		t.Fatalf("Publish failed: %v", err)
	}

	// Subscribe and retrieve the event from NATS
	sub, err := js.SubscribeSync(streamName, nats.DeliverLast())
	if err != nil {
		t.Fatalf("Failed to subscribe: %v", err)
	}
	defer sub.Unsubscribe()

	// Fetch the message
	msg, err := sub.NextMsg(2 * time.Second)
	if err != nil {
		t.Fatalf("Failed to receive message: %v", err)
	}

	// Deserialize the event
	var retrievedEvent model.Event
	if err := json.Unmarshal(msg.Data, &retrievedEvent); err != nil {
		t.Fatalf("Failed to unmarshal event: %v", err)
	}

	// Verify event fields
	if retrievedEvent.EventID != result.EventID {
		t.Errorf("EventID = %q, want %q", retrievedEvent.EventID, result.EventID)
	}
	if retrievedEvent.Stream != streamName {
		t.Errorf("Stream = %q, want %q", retrievedEvent.Stream, streamName)
	}
	if retrievedEvent.Source != "sensor-gateway" {
		t.Errorf("Source = %q, want %q", retrievedEvent.Source, "sensor-gateway")
	}
	if retrievedEvent.Key != "temp-01" {
		t.Errorf("Key = %q, want %q", retrievedEvent.Key, "temp-01")
	}
	if retrievedEvent.Schema != "sensor.reading.v1" {
		t.Errorf("Schema = %q, want %q", retrievedEvent.Schema, "sensor.reading.v1")
	}

	// Verify payload (compare as JSON objects, not strings, because marshaling removes whitespace)
	var originalData, retrievedData map[string]interface{}
	if err := json.Unmarshal(originalPayload, &originalData); err != nil {
		t.Fatalf("Failed to unmarshal original payload: %v", err)
	}
	if err := json.Unmarshal(retrievedEvent.Payload, &retrievedData); err != nil {
		t.Fatalf("Failed to unmarshal retrieved payload: %v", err)
	}

	// Compare specific fields
	if retrievedData["sensor"] != originalData["sensor"] {
		t.Errorf("Payload sensor = %v, want %v", retrievedData["sensor"], originalData["sensor"])
	}
	if retrievedData["value"] != originalData["value"] {
		t.Errorf("Payload value = %v, want %v", retrievedData["value"], originalData["value"])
	}
	if retrievedData["unit"] != originalData["unit"] {
		t.Errorf("Payload unit = %v, want %v", retrievedData["unit"], originalData["unit"])
	}
}

func TestPublish_PreservesExistingEventID(t *testing.T) {
	js := getTestJetStream(t)
	streamManager := streams.NewManager(js)
	pub := NewPublisher(js, streamManager)

	streamName := "test.publish.preserveid"
	t.Cleanup(func() { cleanupStream(t, js, streamName) })

	// Create stream
	config := streams.DefaultStreamConfig(streamName)
	if err := streamManager.CreateStream(config); err != nil {
		t.Fatalf("Failed to create test stream: %v", err)
	}

	// Event with pre-existing eventId
	existingID := model.GenerateEventID()
	event := &model.Event{
		EventID:   existingID,
		Stream:    streamName,
		Source:    "test-producer",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test": "preserve"}`),
	}

	// Publish
	result, err := pub.Publish(event)
	if err != nil {
		t.Fatalf("Publish failed: %v", err)
	}

	// Verify eventId was preserved
	if result.EventID != existingID {
		t.Errorf("EventID changed: got %q, want %q", result.EventID, existingID)
	}
	if event.EventID != existingID {
		t.Errorf("Event EventID changed: got %q, want %q", event.EventID, existingID)
	}
}

func TestMain(m *testing.M) {
	// Check if NATS is available before running tests
	urls := []string{
		"nats://flux-nats:4222",
		"nats://localhost:4223",
	}

	var connected bool
	for _, url := range urls {
		nc, err := nats.Connect(url, nats.Timeout(2*time.Second))
		if err == nil {
			nc.Close()
			connected = true
			break
		}
	}

	if !connected {
		os.Exit(0) // Skip all tests if NATS unavailable
	}

	os.Exit(m.Run())
}
