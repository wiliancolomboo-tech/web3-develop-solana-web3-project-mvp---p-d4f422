# Build BPF bytecode
anchor build

# Sync Program ID with generated keypair
anchor keys sync

# Deploy to Solana Devnet
solana program deploy \
  --program-id deployer-keypair.json \
  target/deploy/vibe_stream.so