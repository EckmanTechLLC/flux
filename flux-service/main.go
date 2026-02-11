package main

import (
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/flux/flux-service/internal/model"
	"github.com/flux/flux-service/internal/publisher"
	"github.com/flux/flux-service/internal/streams"
	"github.com/nats-io/nats.go"
)

func main() {
	natsURL := os.Getenv("NATS_URL")
	if natsURL == "" {
		natsURL = "nats://localhost:4223"
	}

	port := os.Getenv("PORT")
	if port == "" {
		port = "8090"
	}

	log.Printf("Flux Service starting...")
	log.Printf("NATS URL: %s", natsURL)
	log.Printf("Service port: %s", port)

	nc, err := connectToNATS(natsURL)
	if err != nil {
		log.Fatalf("Failed to connect to NATS: %v", err)
	}
	defer nc.Close()

	js, err := nc.JetStream()
	if err != nil {
		log.Fatalf("Failed to get JetStream context: %v", err)
	}

	log.Printf("Connected to NATS successfully")
	log.Printf("JetStream enabled: %v", js != nil)

	if err := verifyJetStream(js); err != nil {
		log.Fatalf("JetStream verification failed: %v", err)
	}

	// Initialize streams
	streamManager := streams.NewManager(js)
	defaultStreams := []string{
		"alarms.events",
		"sensor.readings",
	}

	log.Printf("Initializing streams...")
	if err := streamManager.InitializeStreams(defaultStreams, log.Printf); err != nil {
		log.Printf("Warning: Stream initialization had errors: %v", err)
		// Continue anyway - streams may already exist or can be created later
	}

	// Test publish on startup
	pub := publisher.NewPublisher(js, streamManager)
	testEvent := &model.Event{
		Stream:    "alarms.events",
		Source:    "flux-service-startup",
		Timestamp: time.Now().UnixMilli(),
		Payload:   []byte(`{"message": "Flux service started", "status": "ready"}`),
		Schema:    "service.startup.v1",
	}

	log.Printf("Testing publish API with startup event...")
	result, err := pub.Publish(testEvent)
	if err != nil {
		log.Printf("Warning: Test publish failed: %v", err)
	} else {
		log.Printf("Test publish successful: eventId=%s, sequence=%d", result.EventID, result.Sequence)
	}

	log.Printf("Flux Service ready on port %s", port)
	log.Printf("Press Ctrl+C to shutdown")

	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
	<-sigChan

	log.Printf("Shutting down...")
}

func connectToNATS(url string) (*nats.Conn, error) {
	opts := []nats.Option{
		nats.Name("flux-service"),
		nats.MaxReconnects(-1),
		nats.ReconnectWait(2 * time.Second),
		nats.DisconnectErrHandler(func(nc *nats.Conn, err error) {
			log.Printf("Disconnected from NATS: %v", err)
		}),
		nats.ReconnectHandler(func(nc *nats.Conn) {
			log.Printf("Reconnected to NATS at %s", nc.ConnectedUrl())
		}),
	}

	log.Printf("Connecting to NATS at %s...", url)
	nc, err := nats.Connect(url, opts...)
	if err != nil {
		return nil, fmt.Errorf("connect failed: %w", err)
	}

	return nc, nil
}

func verifyJetStream(js nats.JetStreamContext) error {
	info, err := js.AccountInfo()
	if err != nil {
		return fmt.Errorf("get account info: %w", err)
	}

	log.Printf("JetStream account info:")
	log.Printf("  Memory: %d bytes", info.Memory)
	log.Printf("  Storage: %d bytes", info.Store)
	log.Printf("  Streams: %d", info.Streams)
	log.Printf("  Consumers: %d", info.Consumers)

	return nil
}
