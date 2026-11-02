# Launch integration daemon in background
docker-compose up -d --build

# Inspect live telemetry stream
docker-compose logs -f telemetry-daemon