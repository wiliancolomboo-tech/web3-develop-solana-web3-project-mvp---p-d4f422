# Verify Daemon Healthcheck
curl -s http://localhost:8080/healthz | jq
# Response:
# {
#   "status": "HEALTHY",
#   "programId": "UniVibe1111111111111111111111111111111111111",
#   "uptimeSeconds": 142.3,
#   "timestamp": "2025-02-15T08:00:00.000Z"
# }

# Scrape Prometheus Financial Metrics
curl -s http://localhost:8080/metrics | grep univibe_