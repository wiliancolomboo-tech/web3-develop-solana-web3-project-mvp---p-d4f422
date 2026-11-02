# 1. Build and verify the on-chain Anchor program
anchor build

# 2. Deploy to Solana Devnet
solana program deploy \
  --program-id target/deploy/univibe_escrow-keypair.json \
  target/deploy/univibe_escrow.so \
  --url devnet

# 3. Create environment file
cat <<EOF > .env
SOLANA_RPC_URL=https://api.devnet.solana.com
SOLANA_WS_URL=wss://api.devnet.solana.com
PROGRAM_ID=$(solana address -k target/deploy/univibe_escrow-keypair.json)
ANCHOR_WALLET=./secrets/relayer-keypair.json
SLACK_WEBHOOK_URL=https://api.example.com/webhooks/alerts/placeholder
DISCORD_WEBHOOK_URL=https://api.example.com/webhooks/alerts/placeholder
RESEND_API_KEY=re_live_enterprise_token_sample
ADMIN_EMAIL=grants@university.edu
EOF

# 4. Generate relayer credentials if not present
mkdir -p secrets
solana-keygen new --no-bip39-passphrase -o secrets/relayer-keypair.json --silent

# 5. Boot containerized relayer service
docker-compose up -d --build

# 6. Verify healthcheck and logs
docker-compose logs -f univibe-relayer