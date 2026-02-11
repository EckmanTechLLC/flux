package publisher

import (
	"encoding/json"
	"fmt"
	"log"

	"github.com/flux/flux-service/internal/model"
	"github.com/flux/flux-service/internal/streams"
	"github.com/nats-io/nats.go"
)

// PublishResult contains the result of a successful publish operation
type PublishResult struct {
	EventID  string // UUIDv7 event identifier
	Stream   string // Stream name where event was published
	Sequence uint64 // NATS sequence number
}

// Publisher handles event publishing to NATS JetStream
type Publisher struct {
	js            nats.JetStreamContext
	streamManager *streams.Manager
}

// NewPublisher creates a new publisher instance
func NewPublisher(js nats.JetStreamContext, streamManager *streams.Manager) *Publisher {
	return &Publisher{
		js:            js,
		streamManager: streamManager,
	}
}

// Publish validates an event and publishes it to the appropriate stream.
// It validates the event, ensures the stream exists, publishes to NATS,
// and returns confirmation with eventId and sequence number.
func (p *Publisher) Publish(event *model.Event) (*PublishResult, error) {
	if event == nil {
		return nil, fmt.Errorf("event cannot be nil")
	}

	// Phase 2 placeholder: Authorization check
	// TODO: Implement authorization - check if producer can publish to this stream
	log.Printf("Authorization placeholder: allowing publish to stream '%s' from source '%s'",
		event.Stream, event.Source)

	// Validate event and generate eventId if missing
	if err := event.ValidateAndPrepare(); err != nil {
		return nil, fmt.Errorf("event validation failed: %w", err)
	}

	// Verify stream exists, auto-create if not
	exists, err := p.streamManager.StreamExists(event.Stream)
	if err != nil {
		return nil, fmt.Errorf("failed to check stream existence: %w", err)
	}

	if !exists {
		log.Printf("Stream '%s' does not exist, auto-creating with default config", event.Stream)
		config := streams.DefaultStreamConfig(event.Stream)
		if err := p.streamManager.CreateStream(config); err != nil {
			return nil, fmt.Errorf("failed to auto-create stream '%s': %w", event.Stream, err)
		}
		log.Printf("Stream '%s' auto-created successfully", event.Stream)
	}

	// Serialize event to JSON
	eventJSON, err := json.Marshal(event)
	if err != nil {
		return nil, fmt.Errorf("failed to serialize event: %w", err)
	}

	// Publish to NATS JetStream
	// Subject is the stream name (e.g., "alarms.events")
	pubAck, err := p.js.Publish(event.Stream, eventJSON)
	if err != nil {
		return nil, fmt.Errorf("failed to publish to NATS: %w", err)
	}

	// Return confirmation
	result := &PublishResult{
		EventID:  event.EventID,
		Stream:   event.Stream,
		Sequence: pubAck.Sequence,
	}

	log.Printf("Published event %s to stream %s (sequence: %d)",
		result.EventID, result.Stream, result.Sequence)

	return result, nil
}
