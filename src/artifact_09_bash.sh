# 1. Build smart contract binaries
anchor build

# 2. Synchronize Program ID
solana address -k target/deploy/vibestream-keypair.json
# (Paste output into declare_id! and Anchor.toml)

# 3. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 4. Run automated test suite
anchor test

# 5. Launch turnkey automation daemon via Docker Compose
export RESEND_API_KEY="re_live_..."
export DISCORD_WEBHOOK_URL="https://api.example.com/webhooks/alerts/placeholder"
export DEFAULT_BUILDER_EMAIL="builder@vibestream.edu"

docker compose up -d --build
docker compose logs -f vibestream-agent