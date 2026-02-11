package model

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strings"

	"github.com/google/uuid"
)

var (
	// streamNamePattern enforces lowercase alphanumeric with dots for hierarchy
	streamNamePattern = regexp.MustCompile(`^[a-z0-9]+(\.[a-z0-9]+)*$`)
)

// ValidationError represents an event validation failure.
type ValidationError struct {
	Field   string
	Message string
}

func (e ValidationError) Error() string {
	return fmt.Sprintf("validation error: %s: %s", e.Field, e.Message)
}

// Validate checks that the event conforms to Flux envelope requirements.
// Returns an error if validation fails.
func (e *Event) Validate() error {
	// Validate required field: stream
	if e.Stream == "" {
		return ValidationError{Field: "stream", Message: "required field is missing"}
	}
	if !streamNamePattern.MatchString(e.Stream) {
		return ValidationError{
			Field:   "stream",
			Message: "must be lowercase alphanumeric with dots (e.g., 'alarms.events')",
		}
	}

	// Validate required field: source
	if e.Source == "" {
		return ValidationError{Field: "source", Message: "required field is missing"}
	}
	if strings.TrimSpace(e.Source) == "" {
		return ValidationError{Field: "source", Message: "cannot be empty or whitespace"}
	}

	// Validate required field: timestamp
	if e.Timestamp <= 0 {
		return ValidationError{Field: "timestamp", Message: "must be a positive Unix epoch milliseconds value"}
	}

	// Validate required field: payload
	if len(e.Payload) == 0 {
		return ValidationError{Field: "payload", Message: "required field is missing"}
	}

	// Validate payload is valid JSON object
	var payloadObj map[string]interface{}
	if err := json.Unmarshal(e.Payload, &payloadObj); err != nil {
		return ValidationError{Field: "payload", Message: fmt.Sprintf("must be valid JSON object: %v", err)}
	}

	// Validate eventId format if provided
	if e.EventID != "" {
		if _, err := uuid.Parse(e.EventID); err != nil {
			return ValidationError{Field: "eventId", Message: "must be a valid UUID"}
		}
	}

	return nil
}

// ValidateAndPrepare validates the event and generates missing fields.
// This is the primary function to call before publishing an event.
func (e *Event) ValidateAndPrepare() error {
	// Generate eventId if not provided
	if e.EventID == "" {
		e.EventID = GenerateEventID()
	}

	// Validate all fields
	return e.Validate()
}
