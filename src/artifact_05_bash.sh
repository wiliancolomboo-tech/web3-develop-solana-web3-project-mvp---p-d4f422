# 1. Clone repository and install dependencies
git clone https://github.com/univibe-protocol/univibe-core.git
cd univibe-core
npm install

# 2. Build Anchor smart contract binaries
anchor build

# 3. Synchronize program keys and deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 3
anchor deploy --provider.cluster devnet

# 4. Initialize Protocol and Launch Enterprise Relay Daemon
cp .env.example .env
npm run start:worker