#!/usr/bin/env bash
# Random data publisher for Flux testing

FLUX_URL="${FLUX_URL:-http://localhost:3000}"
INTERVAL="${INTERVAL:-2}"  # seconds between publishes

echo "Publishing random sensor data to Flux..."
echo "URL: $FLUX_URL"
echo "Interval: ${INTERVAL}s"
echo "Press Ctrl+C to stop"
echo ""

while true; do
    # Generate random data
    SENSOR_ID="sensor-$(shuf -i 1-5 -n 1)"
    TEMP=$(awk -v min=18 -v max=28 'BEGIN{srand(); print min+rand()*(max-min)}')
    HUMIDITY=$(shuf -i 40-80 -n 1)
    TIMESTAMP=$(date +%s)000

    # Create payload
    PAYLOAD=$(cat <<EOF
{
  "stream": "sensors",
  "source": "random-publisher",
  "timestamp": ${TIMESTAMP},
  "payload": {
    "entity_id": "${SENSOR_ID}",
    "properties": {
      "temperature": ${TEMP},
      "humidity": ${HUMIDITY},
      "status": "active"
    }
  }
}
EOF
)

    # Publish to Flux
    RESPONSE=$(curl -s -X POST "${FLUX_URL}/api/events" \
        -H "Content-Type: application/json" \
        -d "$PAYLOAD")

    # Extract eventId from response
    EVENT_ID=$(echo "$RESPONSE" | grep -o '"eventId":"[^"]*"' | cut -d'"' -f4)

    echo "[$(date '+%H:%M:%S')] Published ${SENSOR_ID}: temp=${TEMP}°C, humidity=${HUMIDITY}% → ${EVENT_ID:0:8}..."

    sleep "$INTERVAL"
done
