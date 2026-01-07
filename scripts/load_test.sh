#!/bin/bash

# Simple load test for Experiment Data Plane

BASE_URL="${BASE_URL:-http://localhost:8080}"
DURATION="${DURATION:-60}"
CONCURRENCY="${CONCURRENCY:-100}"

echo "Load Testing Experiment Data Plane"
echo "==================================="
echo "Target: $BASE_URL"
echo "Duration: ${DURATION}s"
echo "Concurrency: $CONCURRENCY"
echo ""

# Check if hey is installed
if ! command -v hey &> /dev/null; then
    echo "Error: 'hey' is not installed."
    echo "Install with: go install github.com/rakyll/hey@latest"
    exit 1
fi

# Create request body
cat > /tmp/experiment_request.json <<EOF
{
  "service": "ranker_svc",
  "hash_keys": {
    "user_id": "user_12345"
  }
}
EOF

echo "Running load test..."
hey -z ${DURATION}s -c ${CONCURRENCY} \
  -m POST \
  -H "Content-Type: application/json" \
  -D /tmp/experiment_request.json \
  "$BASE_URL/experiment"

# Cleanup
rm -f /tmp/experiment_request.json

echo ""
echo "Load test complete!"
echo "Check metrics at: $BASE_URL/metrics"
