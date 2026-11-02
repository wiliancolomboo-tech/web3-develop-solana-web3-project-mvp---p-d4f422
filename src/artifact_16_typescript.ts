import fetch from "node-fetch";
import { CONFIG } from "../config";

export async function broadcastToWebhook(title: string, description: string, fields: { name: string; value: string; inline?: boolean }[]) {
  if (!CONFIG.DISCORD_WEBHOOK_URL) {
    console.warn("[WebhookService] DISCORD_WEBHOOK_URL is unset. Skipping broadcast.");
    return;
  }

  const payload = {
    username: "UniVibe Oracle Bot",
    avatar_url: "https://solana.com/src/img/branding/solanaLogoMark.png",
    embeds: [
      {
        title,
        description,
        color: 0x6366f1,
        fields,
        footer: {
          text: "UniVibe Milestone Streamer • Solana Devnet/Mainnet",
        },
        timestamp: new Date().toISOString(),
      },
    ],
  };

  try {
    const response = await fetch(CONFIG.DISCORD_WEBHOOK_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) {
      console.error(`[WebhookService] HTTP error: ${response.status} ${response.statusText}`);
    } else {
      console.log("[WebhookService] Discord webhook published successfully.");
    }
  } catch (err) {
    console.error("[WebhookService] Failed to emit webhook:", err);
  }
}