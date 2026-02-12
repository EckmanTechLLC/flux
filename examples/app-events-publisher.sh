#!/usr/bin/env bash
# Application events publisher for Flux testing

FLUX_URL="${FLUX_URL:-http://localhost:3000}"
MIN_INTERVAL="${MIN_INTERVAL:-1}"   # min seconds between events
MAX_INTERVAL="${MAX_INTERVAL:-5}"   # max seconds between events

# Event types and their relative frequencies
declare -A EVENT_TYPES=(
    ["user-login"]=30
    ["page-view"]=40
    ["api-call"]=50
    ["payment-processed"]=10
    ["error-occurred"]=5
    ["session-timeout"]=8
    ["cache-miss"]=15
    ["db-query"]=35
)

# Severity levels
declare -A SEVERITIES=(
    ["user-login"]="info"
    ["page-view"]="info"
    ["api-call"]="info"
    ["payment-processed"]="info"
    ["error-occurred"]="error"
    ["session-timeout"]="warning"
    ["cache-miss"]="info"
    ["db-query"]="info"
)

echo "Publishing application events to Flux..."
echo "URL: $FLUX_URL"
echo "Pattern: Bursty (${MIN_INTERVAL}-${MAX_INTERVAL}s intervals)"
echo "Press Ctrl+C to stop"
echo ""

EVENT_COUNT=0

while true; do
    # Weighted random event selection
    TOTAL_WEIGHT=0
    for weight in "${EVENT_TYPES[@]}"; do
        TOTAL_WEIGHT=$((TOTAL_WEIGHT + weight))
    done

    RAND=$((RANDOM % TOTAL_WEIGHT))
    CUMULATIVE=0
    SELECTED_EVENT=""

    for event in "${!EVENT_TYPES[@]}"; do
        CUMULATIVE=$((CUMULATIVE + EVENT_TYPES[$event]))
        if [ $RAND -lt $CUMULATIVE ]; then
            SELECTED_EVENT=$event
            break
        fi
    done

    # Generate event details
    EVENT_COUNT=$((EVENT_COUNT + 1))
    USER_ID="user-$(shuf -i 1001-1999 -n 1)"
    TIMESTAMP=$(date +%s)000
    SEVERITY="${SEVERITIES[$SELECTED_EVENT]}"

    # Random duration for API calls and queries
    DURATION=$(awk -v min=10 -v max=500 'BEGIN{srand(); print int(min+rand()*(max-min))}')

    # Create payload
    PAYLOAD=$(cat <<EOF
{
  "stream": "application",
  "source": "app-monitor",
  "timestamp": ${TIMESTAMP},
  "payload": {
    "entity_id": "app-events",
    "properties": {
      "event_type": "${SELECTED_EVENT}",
      "severity": "${SEVERITY}",
      "user_id": "${USER_ID}",
      "duration_ms": ${DURATION},
      "event_count": ${EVENT_COUNT}
    }
  }
}
EOF
)

    # Publish to Flux
    RESPONSE=$(curl -s -X POST "${FLUX_URL}/api/events" \
        -H "Content-Type: application/json" \
        -d "$PAYLOAD")

    EVENT_ID=$(echo "$RESPONSE" | grep -o '"eventId":"[^"]*"' | cut -d'"' -f4)

    # Color code by severity
    case $SEVERITY in
        error) COLOR="\033[1;31m" ;;    # Red
        warning) COLOR="\033[1;33m" ;;  # Yellow
        info) COLOR="\033[1;36m" ;;     # Cyan
        *) COLOR="\033[0m" ;;
    esac
    RESET="\033[0m"

    printf "[$(date '+%H:%M:%S')] ${COLOR}%-7s${RESET} %-20s %s (%dms) → %.8s...\n" \
        "$SEVERITY" "$SELECTED_EVENT" "$USER_ID" "$DURATION" "$EVENT_ID"

    # Random interval (bursty pattern)
    SLEEP=$(shuf -i ${MIN_INTERVAL}-${MAX_INTERVAL} -n 1)
    sleep "$SLEEP"
done
