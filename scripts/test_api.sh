#!/bin/bash

# Experiment Data Plane API Test Script

BASE_URL="${BASE_URL:-http://localhost:8080}"

echo "Testing Experiment Data Plane API at $BASE_URL"
echo "================================================"

# Health check
echo -e "\n1. Health Check"
echo "GET /health"
curl -s "$BASE_URL/health" | jq .

# List layers
echo -e "\n2. List Layers"
echo "GET /layers"
curl -s "$BASE_URL/layers" | jq .

# Get specific layer
echo -e "\n3. Get Layer Detail"
echo "GET /layers/click_experiment"
curl -s "$BASE_URL/layers/click_experiment" | jq .

# Query experiment parameters - ranker service
echo -e "\n4. Query Experiment (ranker_svc)"
echo "POST /experiment"
curl -s -X POST "$BASE_URL/experiment" \
  -H "Content-Type: application/json" \
  -d '{
    "service": "ranker_svc",
    "hash_keys": {
      "user_id": "user_12345"
    }
  }' | jq .

# Query experiment parameters - search service
echo -e "\n5. Query Experiment (search_svc)"
echo "POST /experiment"
curl -s -X POST "$BASE_URL/experiment" \
  -H "Content-Type: application/json" \
  -d '{
    "service": "search_svc",
    "hash_keys": {
      "session_id": "session_67890"
    }
  }' | jq .

# Query with specific layers
echo -e "\n6. Query with Specific Layers"
echo "POST /experiment"
curl -s -X POST "$BASE_URL/experiment" \
  -H "Content-Type: application/json" \
  -d '{
    "service": "ranker_svc",
    "hash_keys": {
      "user_id": "user_99999"
    },
    "layers": ["click_experiment"]
  }' | jq .

# Test different users for distribution
echo -e "\n7. Test Traffic Distribution (10 users)"
echo "POST /experiment (multiple users)"
for i in {0..9}; do
  response=$(curl -s -X POST "$BASE_URL/experiment" \
    -H "Content-Type: application/json" \
    -d "{
      \"service\": \"ranker_svc\",
      \"hash_keys\": {
        \"user_id\": \"user_$i\"
      }
    }")
  
  algorithm=$(echo "$response" | jq -r '.parameters.algorithm // "none"')
  echo "user_$i -> algorithm: $algorithm"
done

# Metrics
echo -e "\n8. Prometheus Metrics"
echo "GET /metrics"
curl -s "$BASE_URL/metrics" | head -n 20

echo -e "\n================================================"
echo "API Tests Complete"
