# Export production webhook secrets
export RESEND_API_KEY="re_test_000000000000000000000000"
export ENTERPRISE_DISCORD_WEBHOOK="https://api.example.com/webhooks/alerts/placeholder"

# Launch Dockerized worker
docker-compose up -d --build

# Verify operational telemetry
curl -i http://localhost:8080/healthz
curl -i http://localhost:8080/metrics