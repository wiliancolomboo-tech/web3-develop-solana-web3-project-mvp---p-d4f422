# 1. Install Anchor 0.30+ & Solana CLI 1.18+
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install 0.30.1 && avm use 0.30.1

# 2. Build smart contract artifacts
anchor build

# 3. Synchronize Program ID with target keypair
solana-keygen new -o target/deploy/campus_flow-keypair.json --no-bip39-passphrase --force
anchor keys sync

# 4. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 4
anchor deploy --provider.cluster devnet

# 5. Execute Verification Run
anchor test --skip-local-validator