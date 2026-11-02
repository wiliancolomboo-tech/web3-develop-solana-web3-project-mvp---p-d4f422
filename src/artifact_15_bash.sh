# -------------------------------------------------------------
# 1. BUILD & DEPLOY SMART CONTRACT TO SOLANA DEVNET
# -------------------------------------------------------------
# Ensure Solana CLI is on devnet
solana config set --url https://api.devnet.solana.com

# Build the Anchor program binary
anchor build

# Deploy program (Using keypair generated in target/deploy)
solana program deploy \
  --program-id target/deploy/univibe_core-keypair.json \
  target/deploy/univibe_core.so

# -------------------------------------------------------------
# 2. RUN AUTOMATED TEST MATRIX
# -------------------------------------------------------------
yarn install
anchor test

# -------------------------------------------------------------
# 3. SPIN UP ENTERPRISE NOTIFICATION WORKER
# -------------------------------------------------------------
cp .env.example .env
# Edit .env with your RESEND_API_KEY and DISCORD_WEBHOOK_URL
docker compose up -d --build

# Verify container telemetry
docker logs -f univibe_daemon_production