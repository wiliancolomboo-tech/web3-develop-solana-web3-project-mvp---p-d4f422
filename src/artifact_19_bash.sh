# 1. Clone repository and install dependencies
git clone https://github.com/univibe-protocol/univibe-core.git
cd univibe-core
npm install

# 2. Build Anchor Smart Program
anchor build

# 3. Synchronize program ID into Anchor.toml and lib.rs
anchor keys sync

# 4. Deploy to Solana Devnet (ensure you have devnet SOL)
solana airdrop 2 -u devnet
anchor deploy --provider.cluster devnet

# 5. Run complete QA test suite
anchor test