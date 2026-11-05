# 1. Setup Solana CLI Configuration
solana config set --url https://api.devnet.solana.com
solana airdrop 2

# 2. Build Anchor Smart Program
anchor build

# 3. Synchronize Program ID and Deploy
solana program deploy target/deploy/univibe_core.so

# 4. Run Comprehensive Verification Test Suite
anchor test --skip-local-validator

# 5. Launch Enterprise Relay Daemon via Docker Compose
export RESEND_API_KEY="re_test_000000000000000000000000"
export SLACK_WEBHOOK_URL="https://api.example.com/webhooks/alerts/placeholder"
docker-compose up -d --build
docker-compose logs -f