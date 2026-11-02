import dotenv from "dotenv";
dotenv.config();

export const CONFIG = {
  SOLANA_RPC_URL: process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com",
  SOLANA_WS_URL: process.env.SOLANA_WS_URL || "wss://api.devnet.solana.com",
  PROGRAM_ID: process.env.PROGRAM_ID || "VibeGRNT11111111111111111111111111111111111",
  RESEND_API_KEY: process.env.RESEND_API_KEY || "",
  SENDER_EMAIL: process.env.SENDER_EMAIL || "grants@univibe.network",
  ADMIN_EMAIL: process.env.ADMIN_EMAIL || "team@campusvibe.xyz",
  DISCORD_WEBHOOK_URL: process.env.DISCORD_WEBHOOK_URL || "",
  PORT: parseInt(process.env.PORT || "8080", 10),
};