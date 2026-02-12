#!/usr/bin/env bash
# System metrics publisher for Flux testing

FLUX_URL="${FLUX_URL:-http://localhost:3000}"
INTERVAL="${INTERVAL:-5}"  # seconds between publishes
NUM_HOSTS="${NUM_HOSTS:-3}"  # simulate this many hosts

echo "Publishing system metrics to Flux..."
echo "URL: $FLUX_URL"
echo "Interval: ${INTERVAL}s"
echo "Hosts: $NUM_HOSTS"
echo "Press Ctrl+C to stop"
echo ""

while true; do
    # Rotate through hosts
    HOST_ID="host-0$(shuf -i 1-${NUM_HOSTS} -n 1)"

    # Generate realistic system metrics
    CPU=$(awk -v min=5 -v max=95 'BEGIN{srand(); print min+rand()*(max-min)}')
    MEMORY=$(awk -v min=30 -v max=85 'BEGIN{srand(); print min+rand()*(max-min)}')
    DISK=$(awk -v min=40 -v max=75 'BEGIN{srand(); print min+rand()*(max-min)}')
    LOAD=$(awk -v min=0.1 -v max=4.0 'BEGIN{srand(); print min+rand()*(max-min)}')
    TIMESTAMP=$(date +%s)000

    # Determine status based on metrics
    if (( $(echo "$CPU > 90 || $MEMORY > 90" | bc -l) )); then
        STATUS="warning"
    elif (( $(echo "$CPU > 80 || $MEMORY > 80" | bc -l) )); then
        STATUS="elevated"
    else
        STATUS="healthy"
    fi

    # Create payload
    PAYLOAD=$(cat <<EOF
{
  "stream": "infrastructure",
  "source": "system-monitor",
  "timestamp": ${TIMESTAMP},
  "payload": {
    "entity_id": "${HOST_ID}",
    "properties": {
      "cpu_percent": ${CPU},
      "memory_percent": ${MEMORY},
      "disk_percent": ${DISK},
      "load_avg": ${LOAD},
      "status": "${STATUS}"
    }
  }
}
EOF
)

    # Publish to Flux
    RESPONSE=$(curl -s -X POST "${FLUX_URL}/api/events" \
        -H "Content-Type: application/json" \
        -d "$PAYLOAD")

    # Extract eventId
    EVENT_ID=$(echo "$RESPONSE" | grep -o '"eventId":"[^"]*"' | cut -d'"' -f4)

    # Color code status
    case $STATUS in
        warning) COLOR="\033[1;31m" ;;   # Red
        elevated) COLOR="\033[1;33m" ;;  # Yellow
        healthy) COLOR="\033[1;32m" ;;   # Green
        *) COLOR="\033[0m" ;;
    esac
    RESET="\033[0m"

    printf "[$(date '+%H:%M:%S')] ${COLOR}%-8s${RESET} %-7s: CPU=%5.1f%% MEM=%5.1f%% DISK=%5.1f%% LOAD=%.2f → %.8s...\n" \
        "$STATUS" "$HOST_ID" "$CPU" "$MEMORY" "$DISK" "$LOAD" "$EVENT_ID"

    sleep "$INTERVAL"
done
