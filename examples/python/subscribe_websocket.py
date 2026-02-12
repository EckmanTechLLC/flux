#!/usr/bin/env python3
"""
Flux WebSocket Subscriber
Subscribe to real-time state updates from Flux.
"""

import argparse
import json
import os
import sys
import time
from typing import Optional

import websockets.sync.client as ws_client
from websockets.exceptions import WebSocketException


def format_update(update: dict) -> str:
    """Format state update for display."""
    update_type = update.get("type", "unknown")

    if update_type == "update":
        entity = update.get("entity", {})
        entity_id = entity.get("id", "unknown")
        properties = entity.get("properties", {})
        last_updated = entity.get("lastUpdated", "")

        # Format properties compactly
        prop_str = ", ".join(f"{k}={v}" for k, v in properties.items())

        return f"[{last_updated[:19]}] {entity_id}: {prop_str}"

    elif update_type == "snapshot":
        entity = update.get("entity", {})
        entity_id = entity.get("id", "unknown")
        properties = entity.get("properties", {})

        return f"[SNAPSHOT] {entity_id}: {json.dumps(properties)}"

    else:
        return json.dumps(update, indent=2)


def subscribe(
    flux_url: str,
    entity_id: Optional[str] = None,
    verbose: bool = False,
):
    """
    Subscribe to state updates via WebSocket.

    Args:
        flux_url: Flux base URL (http://localhost:3000)
        entity_id: Optional specific entity to subscribe to (None = all entities)
        verbose: Print raw JSON messages
    """
    # Convert HTTP URL to WebSocket URL
    ws_url = flux_url.replace("http://", "ws://").replace("https://", "wss://")
    ws_url = f"{ws_url}/api/ws"

    print(f"Connecting to Flux: {ws_url}")
    if entity_id:
        print(f"Subscribing to entity: {entity_id}")
    else:
        print("Subscribing to all entities")
    print("Press Ctrl+C to stop\n")

    try:
        with ws_client.connect(ws_url, open_timeout=10) as websocket:
            print("✓ Connected\n")

            # Send subscription message
            subscribe_msg = {"type": "subscribe"}
            if entity_id:
                subscribe_msg["entityId"] = entity_id

            websocket.send(json.dumps(subscribe_msg))

            if verbose:
                print(f"→ Sent: {json.dumps(subscribe_msg)}\n")

            # Receive and display updates
            while True:
                try:
                    message = websocket.recv(timeout=1.0)

                    if verbose:
                        print(f"← Received: {message}\n")

                    try:
                        update = json.loads(message)
                        print(format_update(update))
                    except json.JSONDecodeError:
                        print(f"Warning: Invalid JSON: {message}", file=sys.stderr)

                except TimeoutError:
                    # No message received, continue (allows Ctrl+C to work)
                    continue

    except KeyboardInterrupt:
        print("\n\nDisconnected (Ctrl+C)")
        sys.exit(0)

    except ConnectionRefusedError:
        print(f"Error: Cannot connect to Flux at {flux_url}", file=sys.stderr)
        print("Is Flux running? Try: docker-compose up -d", file=sys.stderr)
        sys.exit(1)

    except TimeoutError:
        print(f"Error: Connection to {flux_url} timed out", file=sys.stderr)
        sys.exit(1)

    except WebSocketException as e:
        print(f"Error: WebSocket error: {e}", file=sys.stderr)
        sys.exit(1)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description="Subscribe to Flux state updates via WebSocket",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Subscribe to all entities
  %(prog)s

  # Subscribe to specific entity
  %(prog)s --entity sensor-1

  # Verbose mode (show raw JSON)
  %(prog)s --entity sensor-1 --verbose

  # Custom Flux URL
  FLUX_URL=http://flux.example.com:3000 %(prog)s
        """,
    )

    parser.add_argument(
        "--entity",
        help="Subscribe to specific entity (default: all entities)",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Show raw JSON messages",
    )

    args = parser.parse_args()

    # Get Flux URL from environment or use default
    flux_url = os.environ.get("FLUX_URL", "http://localhost:3000")

    # Subscribe
    subscribe(
        flux_url=flux_url,
        entity_id=args.entity,
        verbose=args.verbose,
    )


if __name__ == "__main__":
    main()
