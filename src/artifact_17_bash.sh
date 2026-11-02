# Verify Rust and Solana CLI installations
solana --version
anchor --version

# Target Solana Devnet
solana config set --url https://api.devnet.solana.com

# Create/Confirm deployer keypair
solana-keygen new --outfile deployer-keypair.json --no-bip39-passphrase --force
solana airdrop 2 $(solana address -k deployer-keypair.json) --url https://api.devnet.solana.com