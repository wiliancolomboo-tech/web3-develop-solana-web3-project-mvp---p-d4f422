# 1. Build contract and generate IDL
anchor build

# 2. Sync program key to declare_id
solana-keygen pubkey target/deploy/campus_vibe-keypair.json
anchor keys sync

# 3. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 4. Install client dependencies & launch service
npm install
npm run build
npm start