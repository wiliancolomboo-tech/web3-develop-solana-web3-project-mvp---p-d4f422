import dotenv from "dotenv";
import { VibeStreamIndexer } from "./agent";

dotenv.config();

const config = {
  rpcUrl: process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com",
  wsUrl: process.env.SOLANA_WS_URL || undefined,
  programId: process.env.PROGRAM_ID || "V1beS1ream111111111111111111111111111111111",
  resendApiKey: process.env.RESEND_API_KEY || "",
  discordWebhookUrl: process.env.DISCORD_WEBHOOK_URL || "",
  senderEmail: process.env.NOTIFICATION_SENDER_EMAIL || "alerts@vibestream.org",
  adminEmail: process.env.NOTIFICATION_ADMIN_EMAIL || "grants-committee@vibestream.org",
  port: parseInt(process.env.PORT || "8080", 10),
};

const indexer = new VibeStreamIndexer(config);
indexer.start().catch((err) => {
  console.error("Fatal startup error in VibeStream daemon:", err);
  process.exit(1);
});