# 1. Build Solana Program in Anchor Docker or CLI
anchor build

# 2. Sync Program Keys
solana-keygen pubkey target/deploy/vibestream-keypair.json
# Ensure declare_id! in src/lib.rs matches this pubkey

# 3. Configure Network to Solana Devnet
solana config set --url https://api.devnet.solana.com

# 4. Airdrop Deployment Funds
solana airdrop 2

# 5. Deploy Anchor Program
anchor deploy --provider.cluster devnet

# 6. Verify Deployment On-Chain
solana program show <PROGRAM_ID>