#!/usr/bin/env bash
set -euo pipefail

echo "=========================================================="
echo "    VIBESTREAM PROTOCOL: DEVNET / MAINNET DEPLOYMENT      "
echo "=========================================================="

CLUSTER=${1:-devnet}
echo "Target Cluster: $CLUSTER"

# 1. Build optimized BPF program
echo "[1/4] Compiling Anchor BPF bytecode..."
anchor build

# 2. Get Program Public Key
PROGRAM_KEYPAIR="target/deploy/vibestream-keypair.json"
PROGRAM_ID=$(solana address -k "$PROGRAM_KEYPAIR")
echo "Program ID: $PROGRAM_ID"

# 3. Synchronize program ID across source code
echo "[2/4] Syncing Anchor Program IDs..."
sed -i "s/declare_id!\(\".*\"\);/declare_id!(\"$PROGRAM_ID\");/g" programs/vibestream/src/lib.rs

# 4. Rebuild with aligned ID
anchor build

# 5. Deploy to Solana
echo "[3/4] Deploying BPF binary to Solana $CLUSTER..."
solana program deploy \
  --url "$CLUSTER" \
  --program-id "$PROGRAM_KEYPAIR" \
  target/deploy/vibestream.so

# 6. Initialize IDL on-chain
echo "[4/4] Writing on-chain IDL..."
anchor idl init "$PROGRAM_ID" --filepath target/idl/vibestream.json --provider.cluster "$CLUSTER" || \
anchor idl upgrade "$PROGRAM_ID" --filepath target/idl/vibestream.json --provider.cluster "$CLUSTER"

echo "=========================================================="
echo "DEPLOYMENT COMPLETE!"
echo "Program ID: $PROGRAM_ID"
echo "=========================================================="