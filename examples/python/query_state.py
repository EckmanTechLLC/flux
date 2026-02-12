#!/usr/bin/env python3
"""
Flux State Query Client
Query current entity state from Flux via HTTP API.
"""

import argparse
import json
import os
import sys
from typing import Optional

import requests


def query_entity(flux_url: str, entity_id: Optional[str] = None) -> dict:
    """
    Query entity state from Flux.

    Args:
        flux_url: Flux API base URL
        entity_id: Optional specific entity ID (None = all entities)

    Returns:
        Entity or list of entities

    Raises:
        requests.RequestException: On HTTP error
    """
    if entity_id:
        url = f"{flux_url}/api/state/entities/{entity_id}"
    else:
        url = f"{flux_url}/api/state/entities"

    try:
        response = requests.get(url, timeout=10)
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
        if e.response.status_code == 404:
            print(f"Error: Entity '{entity_id}' not found", file=sys.stderr)
            sys.exit(1)
        else:
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


def format_entity(entity: dict, compact: bool = False) -> str:
    """Format entity for display."""
    entity_id = entity.get("id", "unknown")
    properties = entity.get("properties", {})
    last_updated = entity.get("lastUpdated", "")

    if compact:
        # One-line format
        prop_str = ", ".join(f"{k}={v}" for k, v in properties.items())
        return f"{entity_id}: {prop_str} (updated: {last_updated[:19]})"
    else:
        # Multi-line format with JSON
        return f"""Entity: {entity_id}
Last Updated: {last_updated}
Properties:
{json.dumps(properties, indent=2)}
"""


def main():
    parser = argparse.ArgumentParser(
        description="Query entity state from Flux",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # List all entities
  %(prog)s

  # Get specific entity
  %(prog)s --entity sensor-1

  # Compact output
  %(prog)s --compact

  # Raw JSON output
  %(prog)s --json

  # Custom Flux URL
  FLUX_URL=http://flux.example.com:3000 %(prog)s --entity sensor-1
        """,
    )

    parser.add_argument(
        "--entity",
        help="Query specific entity (default: list all entities)",
    )
    parser.add_argument(
        "--compact",
        "-c",
        action="store_true",
        help="Compact one-line output per entity",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output raw JSON",
    )

    args = parser.parse_args()

    # Get Flux URL from environment or use default
    flux_url = os.environ.get("FLUX_URL", "http://localhost:3000")

    # Query state
    result = query_entity(flux_url=flux_url, entity_id=args.entity)

    # Output
    if args.json:
        # Raw JSON
        print(json.dumps(result, indent=2))
    elif isinstance(result, list):
        # Multiple entities
        if len(result) == 0:
            print("No entities found")
        else:
            print(f"Found {len(result)} entities:\n")
            for entity in result:
                print(format_entity(entity, compact=args.compact))
                if not args.compact:
                    print()
    else:
        # Single entity
        print(format_entity(result, compact=args.compact))


if __name__ == "__main__":
    main()
