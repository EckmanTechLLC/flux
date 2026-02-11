package model

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
)

// TestGenerateEventID verifies UUIDv7 generation
func TestGenerateEventID(t *testing.T) {
	id1 := GenerateEventID()
	id2 := GenerateEventID()

	// Should generate valid UUIDs
	if _, err := uuid.Parse(id1); err != nil {
		t.Errorf("Generated invalid UUID: %v", err)
	}
	if _, err := uuid.Parse(id2); err != nil {
		t.Errorf("Generated invalid UUID: %v", err)
	}

	// Should generate unique IDs
	if id1 == id2 {
		t.Errorf("Generated duplicate UUIDs: %s", id1)
	}
}

// TestEvent_SetTimestampNow verifies timestamp generation
func TestEvent_SetTimestampNow(t *testing.T) {
	event := &Event{}
	before := time.Now().UnixMilli()
	event.SetTimestampNow()
	after := time.Now().UnixMilli()

	if event.Timestamp < before || event.Timestamp > after {
		t.Errorf("Timestamp out of range: got %d, expected between %d and %d",
			event.Timestamp, before, after)
	}
}

// TestEvent_GetTimestamp verifies timestamp parsing
func TestEvent_GetTimestamp(t *testing.T) {
	expected := time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC)
	event := &Event{Timestamp: expected.UnixMilli()}

	result := event.GetTimestamp()
	if result.Unix() != expected.Unix() {
		t.Errorf("GetTimestamp() mismatch: got %v, expected %v", result, expected)
	}
}

// TestValidate_ValidEvent verifies validation passes for valid events
func TestValidate_ValidEvent(t *testing.T) {
	tests := []struct {
		name  string
		event Event
	}{
		{
			name: "complete event with all fields",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "alarms.events",
				Source:    "ignition.gateway.prod",
				Timestamp: time.Now().UnixMilli(),
				Key:       "Area1/Pump3/HighTemp",
				Schema:    "alarm.raise.v1",
				Payload:   json.RawMessage(`{"severity":"high","value":92.4}`),
			},
		},
		{
			name: "minimal event without optional fields",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "sensor.readings",
				Source:    "plant-a.scada",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"temperature":25.5}`),
			},
		},
		{
			name: "hierarchical stream name",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "sensor.hvac.temperature.zone1",
				Source:    "building-automation",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"value":22.3}`),
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.event.Validate(); err != nil {
				t.Errorf("Validate() failed for valid event: %v", err)
			}
		})
	}
}

// TestValidate_MissingRequiredFields verifies validation fails for missing fields
func TestValidate_MissingRequiredFields(t *testing.T) {
	tests := []struct {
		name      string
		event     Event
		wantField string
	}{
		{
			name: "missing stream",
			event: Event{
				EventID:   GenerateEventID(),
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "stream",
		},
		{
			name: "missing source",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "test.stream",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "source",
		},
		{
			name: "missing timestamp",
			event: Event{
				EventID: GenerateEventID(),
				Stream:  "test.stream",
				Source:  "test-source",
				Payload: json.RawMessage(`{"test":"data"}`),
			},
			wantField: "timestamp",
		},
		{
			name: "missing payload",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "test.stream",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
			},
			wantField: "payload",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.event.Validate()
			if err == nil {
				t.Errorf("Validate() should fail for missing %s", tt.wantField)
				return
			}
			verr, ok := err.(ValidationError)
			if !ok {
				t.Errorf("Expected ValidationError, got %T", err)
				return
			}
			if verr.Field != tt.wantField {
				t.Errorf("Expected error for field %s, got %s", tt.wantField, verr.Field)
			}
		})
	}
}

// TestValidate_InvalidFormats verifies validation fails for invalid formats
func TestValidate_InvalidFormats(t *testing.T) {
	tests := []struct {
		name      string
		event     Event
		wantField string
	}{
		{
			name: "stream with uppercase",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "Alarms.Events",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "stream",
		},
		{
			name: "stream with spaces",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "alarms events",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "stream",
		},
		{
			name: "stream starting with dot",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    ".alarms.events",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "stream",
		},
		{
			name: "source with only whitespace",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "test.stream",
				Source:    "   ",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "source",
		},
		{
			name: "negative timestamp",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "test.stream",
				Source:    "test-source",
				Timestamp: -1,
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "timestamp",
		},
		{
			name: "invalid JSON payload",
			event: Event{
				EventID:   GenerateEventID(),
				Stream:    "test.stream",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{invalid json`),
			},
			wantField: "payload",
		},
		{
			name: "invalid eventId UUID",
			event: Event{
				EventID:   "not-a-uuid",
				Stream:    "test.stream",
				Source:    "test-source",
				Timestamp: time.Now().UnixMilli(),
				Payload:   json.RawMessage(`{"test":"data"}`),
			},
			wantField: "eventId",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.event.Validate()
			if err == nil {
				t.Errorf("Validate() should fail for %s", tt.name)
				return
			}
			verr, ok := err.(ValidationError)
			if !ok {
				t.Errorf("Expected ValidationError, got %T", err)
				return
			}
			if verr.Field != tt.wantField {
				t.Errorf("Expected error for field %s, got %s", tt.wantField, verr.Field)
			}
		})
	}
}

// TestValidateAndPrepare_GeneratesEventID verifies eventId generation
func TestValidateAndPrepare_GeneratesEventID(t *testing.T) {
	event := Event{
		Stream:    "test.stream",
		Source:    "test-source",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test":"data"}`),
	}

	if err := event.ValidateAndPrepare(); err != nil {
		t.Fatalf("ValidateAndPrepare() failed: %v", err)
	}

	if event.EventID == "" {
		t.Error("ValidateAndPrepare() should generate eventId")
	}

	if _, err := uuid.Parse(event.EventID); err != nil {
		t.Errorf("Generated eventId is invalid UUID: %v", err)
	}
}

// TestValidateAndPrepare_PreservesExistingEventID verifies existing eventId is kept
func TestValidateAndPrepare_PreservesExistingEventID(t *testing.T) {
	existingID := GenerateEventID()
	event := Event{
		EventID:   existingID,
		Stream:    "test.stream",
		Source:    "test-source",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test":"data"}`),
	}

	if err := event.ValidateAndPrepare(); err != nil {
		t.Fatalf("ValidateAndPrepare() failed: %v", err)
	}

	if event.EventID != existingID {
		t.Errorf("ValidateAndPrepare() changed eventId: got %s, want %s", event.EventID, existingID)
	}
}

// TestValidateAndPrepare_FailsOnInvalidEvent verifies validation errors are caught
func TestValidateAndPrepare_FailsOnInvalidEvent(t *testing.T) {
	event := Event{
		Stream: "INVALID",
		Source: "test-source",
	}

	err := event.ValidateAndPrepare()
	if err == nil {
		t.Error("ValidateAndPrepare() should fail for invalid event")
	}
}

// TestValidationError_Error verifies error message formatting
func TestValidationError_Error(t *testing.T) {
	err := ValidationError{Field: "stream", Message: "is invalid"}
	expected := "validation error: stream: is invalid"

	if err.Error() != expected {
		t.Errorf("Error() = %q, want %q", err.Error(), expected)
	}
}

// TestEvent_JSONSerialization verifies event marshaling/unmarshaling
func TestEvent_JSONSerialization(t *testing.T) {
	original := Event{
		EventID:   GenerateEventID(),
		Stream:    "test.stream",
		Source:    "test-source",
		Timestamp: time.Now().UnixMilli(),
		Key:       "test-key",
		Schema:    "test.schema.v1",
		Payload:   json.RawMessage(`{"field":"value"}`),
	}

	// Marshal to JSON
	data, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("json.Marshal() failed: %v", err)
	}

	// Unmarshal back
	var decoded Event
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("json.Unmarshal() failed: %v", err)
	}

	// Verify all fields match
	if decoded.EventID != original.EventID {
		t.Errorf("EventID mismatch: got %s, want %s", decoded.EventID, original.EventID)
	}
	if decoded.Stream != original.Stream {
		t.Errorf("Stream mismatch: got %s, want %s", decoded.Stream, original.Stream)
	}
	if decoded.Source != original.Source {
		t.Errorf("Source mismatch: got %s, want %s", decoded.Source, original.Source)
	}
	if decoded.Timestamp != original.Timestamp {
		t.Errorf("Timestamp mismatch: got %d, want %d", decoded.Timestamp, original.Timestamp)
	}
	if decoded.Key != original.Key {
		t.Errorf("Key mismatch: got %s, want %s", decoded.Key, original.Key)
	}
	if decoded.Schema != original.Schema {
		t.Errorf("Schema mismatch: got %s, want %s", decoded.Schema, original.Schema)
	}
	if string(decoded.Payload) != string(original.Payload) {
		t.Errorf("Payload mismatch: got %s, want %s", decoded.Payload, original.Payload)
	}
}

// TestEvent_OmitsEmptyOptionalFields verifies JSON omitempty behavior
func TestEvent_OmitsEmptyOptionalFields(t *testing.T) {
	event := Event{
		EventID:   GenerateEventID(),
		Stream:    "test.stream",
		Source:    "test-source",
		Timestamp: time.Now().UnixMilli(),
		Payload:   json.RawMessage(`{"test":"data"}`),
	}

	data, err := json.Marshal(event)
	if err != nil {
		t.Fatalf("json.Marshal() failed: %v", err)
	}

	jsonStr := string(data)

	// Optional fields should be omitted when empty
	if strings.Contains(jsonStr, `"key"`) {
		t.Error("Empty key field should be omitted from JSON")
	}
	if strings.Contains(jsonStr, `"schema"`) {
		t.Error("Empty schema field should be omitted from JSON")
	}

	// Required fields should be present
	if !strings.Contains(jsonStr, `"eventId"`) {
		t.Error("eventId field should be present in JSON")
	}
	if !strings.Contains(jsonStr, `"stream"`) {
		t.Error("stream field should be present in JSON")
	}
	if !strings.Contains(jsonStr, `"source"`) {
		t.Error("source field should be present in JSON")
	}
	if !strings.Contains(jsonStr, `"timestamp"`) {
		t.Error("timestamp field should be present in JSON")
	}
	if !strings.Contains(jsonStr, `"payload"`) {
		t.Error("payload field should be present in JSON")
	}
}
