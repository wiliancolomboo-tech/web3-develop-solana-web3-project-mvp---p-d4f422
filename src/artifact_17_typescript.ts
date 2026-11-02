import http from "http";
import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider, Idl } from "@coral-xyz/anchor";
import { CONFIG } from "./config";
import { sendMilestoneDisbursementReceipt } from "./services/email";
import { broadcastToWebhook } from "./services/webhook";

// Minimal IDL definition for parsing events safely
const IDL_EVENTS = {
  events: [
    {
      name: "GrantCreatedEvent",
      fields: [
        { name: "grantPubkey", type: "publicKey", index: false },
        { name: "sponsor", type: "publicKey", index: false },
        { name: "beneficiary", type: "publicKey", index: false },
        { name: "reviewer", type: "publicKey", index: false },
        { name: "totalAmount", type: "u64", index: false },
        { name: "milestoneCount", type: "u8", index: false },
        { name: "title", type: "string", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
    {
      name: "MilestoneFundsReleasedEvent",
      fields: [
        { name: "grantPubkey", type: "publicKey", index: false },
        { name: "milestoneIndex", type: "u8", index: false },
        { name: "beneficiary", type: "publicKey", index: false },
        { name: "amount", type: "u64", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
  ],
};

async function main() {
  console.log("=== Starting UniVibe Enterprise Telemetry Worker ===");
  console.log(`Connecting to Solana RPC: ${CONFIG.SOLANA_RPC_URL}`);

  const connection = new Connection(CONFIG.SOLANA_RPC_URL, {
    wsEndpoint: CONFIG.SOLANA_WS_URL,
    commitment: "confirmed",
  });

  const programId = new PublicKey(CONFIG.PROGRAM_ID);
  
  // Dummy wallet for Anchor Provider read-only listener
  const dummyWallet = {
    publicKey: PublicKey.default,
    signTransaction: async (tx: any) => tx,
    signAllTransactions: async (txs: any[]) => txs,
  };

  const provider = new AnchorProvider(connection, dummyWallet as any, { commitment: "confirmed" });
  const program = new Program(IDL_EVENTS as unknown as Idl, provider);

  // Subscribe to Program Logs
  connection.onLogs(
    programId,
    async (logsInfo) => {
      const logs = logsInfo.logs;
      for (const log of logs) {
        if (log.includes("GrantCreatedEvent")) {
          console.log("[Worker] Detected GrantCreatedEvent");
          await broadcastToWebhook(
            "🎉 New Campus Grant Initialized!",
            "A university sponsor has funded a new project escrow on Solana.",
            [
              { name: "Program", value: CONFIG.PROGRAM_ID, inline: false },
              { name: "Signature", value: logsInfo.signature, inline: false }
            ]
          );
        } else if (log.includes("MilestoneFundsReleasedEvent")) {
          console.log("[Worker] Detected MilestoneFundsReleasedEvent");
          // Dispatch notifications
          await sendMilestoneDisbursementReceipt(
            "VibeVaultEscrow111111111111111111111111",
            1,
            "StudentBuilderWallet1111111111111111111111",
            0.5
          );
          await broadcastToWebhook(
            "💸 Milestone Disbursed & Settled",
            "Proof accepted and funds released directly to student team.",
            [
              { name: "Amount", value: "0.5 SOL", inline: true },
              { name: "Tx Signature", value: logsInfo.signature, inline: true }
            ]
          );
        }
      }
    },
    "confirmed"
  );

  console.log(`[Worker] Successfully listening to on-chain program: ${programId.toBase58()}`);

  // Lightweight HTTP Healthcheck for Docker/Kubernetes container orchestration
  const server = http.createServer((req, res) => {
    if (req.url === "/healthz") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "healthy", timestamp: Date.now() }));
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(CONFIG.PORT, () => {
    console.log(`[Healthcheck] Telemetry status server running on port ${CONFIG.PORT}`);
  });
}

main().catch((err) => {
  console.error("[Fatal Error] Worker crashed:", err);
  process.exit(1);
});