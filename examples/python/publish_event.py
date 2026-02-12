#!/usr/bin/env python3
"""
Flux Event Publisher
Publishes events to Flux via HTTP API.
"""

import argparse
import json
import os
import sys
import time
from typing import Any, Dict

import requests


def publish_event(
    flux_url: str,
    stream: str,
    source: str,
    entity_id: str,
    properties: Dict[str, Any],
    key: str = None,
    schema: str = None,
) -> Dict[str, Any]:
    """
    Publish a single event to Flux.

    Args:
        flux_url: Flux API base URL
        stream: Logical stream/namespace
        source: Producer identity
        entity_id: Entity identifier
        properties: Entity properties to update
        key: Optional ordering/grouping key
        schema: Optional schema metadata

    Returns:
        Response from Flux API

    Raises:
        requests.RequestException: On HTTP error
    """
    # Build event payload
    event = {
        "stream": stream,
        "source": source,
        "timestamp": int(time.time() * 1000),  # Unix epoch milliseconds
        "payload": {
            "entity_id": entity_id,
            "properties": properties,
        },
    }

    # Add optional fields
    if key:
        event["key"] = key
    if schema:
        event["schema"] = schema

    # Publish to Flux
    url = f"{flux_url}/api/events"
    headers = {"Content-Type": "application/json"}

    try:
        response = requests.post(url, json=event, headers=headers, timeout=10)
        response.raise_for_status()
        return response.json()
    except requests.exceptions.ConnectionError:
        print(f"Error: Cannot connect to Flux at {flux_url}", file=sys.stderr)
        print("Is Flux running? Try: docker-compose up -d", file=sys.stderr)
        sys.exit(1)
    except requests.exceptions.Timeout:
        print(f"Error: Request to {flux_url} timed out", file=sys.stderr)
        sys.exit(1)
    except requests.exceptions.HTTPError as e:
        print(f"Error: HTTP {e.response.status_code}", file=sys.stderr)
        try:
            error_detail = e.response.json()
            print(f"Details: {json.dumps(error_detail, indent=2)}", file=sys.stderr)
        except:
            print(f"Details: {e.response.text}", file=sys.stderr)
        sys.exit(1)
    except requests.exceptions.RequestException as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def parse_properties(prop_args: list) -> Dict[str, Any]:
    """
    Parse property arguments in the form key=value.
    Attempts to parse as JSON, falls back to string.

    Examples:
        temperature=22.5 -> {"temperature": 22.5}
        active=true -> {"active": true}
        status=online -> {"status": "online"}
    """
    properties = {}

    for prop in prop_args:
        if "=" not in prop:
            print(f"Error: Invalid property format '{prop}'. Use key=value", file=sys.stderr)
            sys.exit(1)

        key, value = prop.split("=", 1)

        # Try to parse as JSON (handles numbers, booleans, null)
        try:
            properties[key] = json.loads(value)
        except json.JSONDecodeError:
            # Fall back to string
            properties[key] = value

    return properties


def main():
    parser = argparse.ArgumentParser(
        description="Publish events to Flux",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Publish sensor reading
  %(prog)s --stream sensors --source demo --entity sensor-1 temperature=22.5 humidity=45

  # With optional key and schema
  %(prog)s --stream sensors --source demo --entity sensor-1 --key sensor-1 --schema v1 temp=22.5

  # Custom Flux URL
  FLUX_URL=http://flux.example.com:3000 %(prog)s --stream test --source cli --entity test-1 value=42
        """,
    )

    parser.add_argument(
        "--stream",
        required=True,
        help="Logical stream/namespace (e.g., sensors, infrastructure, application)",
    )
    parser.add_argument(
        "--source",
        required=True,
        help="Producer identity (e.g., sensor-01, demo, cli)",
    )
    parser.add_argument(
        "--entity",
        required=True,
        help="Entity identifier (e.g., sensor-1, host-01, user-123)",
    )
    parser.add_argument(
        "--key",
        help="Optional ordering/grouping key",
    )
    parser.add_argument(
        "--schema",
        help="Optional schema metadata",
    )
    parser.add_argument(
        "properties",
        nargs="+",
        metavar="key=value",
        help="Entity properties (e.g., temperature=22.5 status=active)",
    )

    args = parser.parse_args()

    # Get Flux URL from environment or use default
    flux_url = os.environ.get("FLUX_URL", "http://localhost:3000")

    # Parse properties
    properties = parse_properties(args.properties)

    # Publish event
    result = publish_event(
        flux_url=flux_url,
        stream=args.stream,
        source=args.source,
        entity_id=args.entity,
        properties=properties,
        key=args.key,
        schema=args.schema,
    )

    # Print result
    event_id = result.get("eventId", "unknown")
    stream = result.get("stream", args.stream)

    print(f"✓ Published to {stream}")
    print(f"  Entity: {args.entity}")
    print(f"  Event ID: {event_id}")
    print(f"  Properties: {json.dumps(properties)}")


if __name__ == "__main__":
    main()
