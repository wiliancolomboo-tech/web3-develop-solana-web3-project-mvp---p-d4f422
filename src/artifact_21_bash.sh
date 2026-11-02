# Export production webhook secrets
export RESEND_API_KEY="re_123456789_enterprise_token"
export ENTERPRISE_DISCORD_WEBHOOK="https://api.example.com/webhooks/alerts/placeholder"

# Launch Dockerized worker
docker-compose up -d --build

# Verify operational telemetry
curl -i http://localhost:8080/healthz
curl -i http://localhost:8080/metrics