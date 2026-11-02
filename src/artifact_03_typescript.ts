import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider, Idl } from "@coral-xyz/anchor";
import { Resend } from "resend";
import axios from "axios";
import http from "http";
import * as dotenv from "dotenv";

dotenv.config();

// Environment Configuration & Validation
const SOLANA_RPC_WS = process.env.SOLANA_RPC_WS || "wss://api.devnet.solana.com";
const SOLANA_RPC_HTTP = process.env.SOLANA_RPC_HTTP || "https://api.devnet.solana.com";
const PROGRAM_ID = new PublicKey(
  process.env.PROGRAM_ID || "V1be7d2MKMz3B8e1Vw9N4Kz8C6uV8hR3XjB2pC4jA9Z"
);
const RESEND_API_KEY = process.env.RESEND_API_KEY || "re_test_mock_123456789";
const DISCORD_WEBHOOK_URL = process.env.DISCORD_WEBHOOK_URL || "";
const PORT = parseInt(process.env.PORT || "8080", 10);

// Initialize Clients
const resend = new Resend(RESEND_API_KEY);
const connection = new Connection(SOLANA_RPC_HTTP, {
  wsEndpoint: SOLANA_RPC_WS,
  commitment: "confirmed",
});

interface HealthStatus {
  status: string;
  uptime: number;
  eventsProcessed: number;
  lastSlotProcessed: number;
}

const telemetry: HealthStatus = {
  status: "ONLINE",
  uptime: Date.now(),
  eventsProcessed: 0,
  lastSlotProcessed: 0,
};

// Minimal Program IDL for Event Parsing
const IDL: Idl = {
  version: "0.1.0",
  name: "univibe_core",
  instructions: [],
  events: [
    {
      name: "CampaignCreatedEvent",
      fields: [
        { name: "campaignId", type: "u64", index: false },
        { name: "campaignPubkey", type: "publicKey", index: false },
        { name: "creator", type: "publicKey", index: false },
        { name: "targetAmount", type: "u64", index: false },
        { name: "name", type: "string", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
    {
      name: "DonationReceivedEvent",
      fields: [
        { name: "campaignPubkey", type: "publicKey", index: false },
        { name: "campaignId", type: "u64", index: false },
        { name: "donor", type: "publicKey", index: false },
        { name: "amount", type: "u64", index: false },
        { name: "totalCollected", type: "u64", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
    {
      name: "MilestoneReleasedEvent",
      fields: [
        { name: "campaignPubkey", type: "publicKey", index: false },
        { name: "campaignId", type: "u64", index: false },
        { name: "milestoneIndex", type: "u8", index: false },
        { name: "amountReleased", type: "u64", index: false },
        { name: "feeDeducted", type: "u64", index: false },
        { name: "nextMilestone", type: "u8", index: false },
        { name: "isCompleted", type: "bool", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
  ],
};

/**
 * Enterprise Dispatcher: Dispatches Discord webhook alert
 */
async function notifyDiscord(title: string, description: string, fields: Array<{ name: string; value: string; inline?: boolean }>) {
  if (!DISCORD_WEBHOOK_URL) {
    console.log(`[DISCORD DISABLED] ${title} - ${description}`);
    return;
  }
  try {
    await axios.post(DISCORD_WEBHOOK_URL, {
      username: "UniVibe Protocol Sentinel",
      avatar_url: "https://cryptologos.cc/logos/solana-sol-logo.png",
      embeds: [
        {
          title,
          description,
          color: 0x14f195, // Solana Emerald Green
          fields,
          footer: { text: "Powered by UniVibe Protocol • Solana Devnet" },
          timestamp: new Date().toISOString(),
        },
      ],
    });
  } catch (err: any) {
    console.error("Failed to post Discord webhook alert:", err.message);
  }
}

/**
 * Enterprise Dispatcher: Dispatches verified email receipt via Resend
 */
async function sendEmailReceipt(recipient: string, campaignName: string, amountSol: number, txSignature: string) {
  try {
    await resend.emails.send({
      from: "UniVibe Protocol <receipts@univibe.network>",
      to: [recipient],
      subject: `Official Grant Receipt: ${amountSol} SOL contribution to ${campaignName}`,
      html: `
        <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px; border: 1px solid #e2e8f0; border-radius: 8px;">
          <h2 style="color: #0f172a;">UniVibe Campus Micro-Endowment Proof</h2>
          <p>Thank you for empowering university builders on Solana!</p>
          <hr style="border: 0; border-top: 1px solid #e2e8f0;" />
          <p><strong>Campaign:</strong> ${campaignName}</p>
          <p><strong>Amount Contributed:</strong> ${amountSol} SOL</p>
          <p><strong>Transaction Signature:</strong> <code style="background: #f1f5f9; padding: 2px 4px;">${txSignature}</code></p>
          <p style="margin-top: 24px; font-size: 12px; color: #64748b;">
            Cryptographically settled on Solana. Valid for campus student project fiscal records.
          </p>
        </div>
      `,
    });
    console.log(`[RESEND EMAIL SENT] Successfully sent tax receipt to ${recipient}`);
  } catch (err: any) {
    console.warn(`[RESEND BYPASS/ERROR] Note: Ensure custom domain is configured in Resend. Err: ${err.message}`);
  }
}

/**
 * Main Event Listener and Blockchain Daemon
 */
async function runWorker() {
  console.log("==================================================");
  console.log("  UniVibe Protocol: Off-Chain Enterprise Worker   ");
  console.log("==================================================");
  console.log(`Listening on Program ID: ${PROGRAM_ID.toBase58()}`);

  const dummyWallet = {
    publicKey: PROGRAM_ID,
    signTransaction: async (tx: any) => tx,
    signAllTransactions: async (txs: any[]) => txs,
  };

  const provider = new AnchorProvider(connection, dummyWallet as any, {
    commitment: "confirmed",
  });
  const program = new Program(IDL, PROGRAM_ID, provider);

  // Subscribe to Program Events
  program.addEventListener("CampaignCreatedEvent", async (event: any, slot: number) => {
    telemetry.eventsProcessed++;
    telemetry.lastSlotProcessed = slot;
    console.log(`[EVENT] CampaignCreated: ${event.name} (ID: ${event.campaignId.toString()})`);

    await notifyDiscord(
      "🚀 New Campus Campaign Launched!",
      `A new university builder campaign was registered on-chain.`,
      [
        { name: "Campaign Name", value: event.name, inline: true },
        { name: "Target Goal", value: `${event.targetAmount.toNumber() / 1e9} SOL`, inline: true },
        { name: "Creator Key", value: `\`${event.creator.toBase58()}\``, inline: false },
        { name: "Campaign Address", value: `\`${event.campaignPubkey.toBase58()}\``, inline: false },
      ]
    );
  });

  program.addEventListener("DonationReceivedEvent", async (event: any, slot: number) => {
    telemetry.eventsProcessed++;
    telemetry.lastSlotProcessed = slot;
    const amountSol = event.amount.toNumber() / 1e9;
    console.log(`[EVENT] DonationReceived: ${amountSol} SOL for Campaign ${event.campaignId.toString()}`);

    await notifyDiscord(
      "💰 Micro-Endowment Donation Received!",
      `Funds successfully deposited into campaign PDA vault.`,
      [
        { name: "Donation Amount", value: `${amountSol} SOL`, inline: true },
        { name: "Campaign ID", value: event.campaignId.toString(), inline: true },
        { name: "Donor", value: `\`${event.donor.toBase58()}\``, inline: false },
        { name: "Vault Total", value: `${event.totalCollected.toNumber() / 1e9} SOL`, inline: true },
      ]
    );

    // Mock receipt trigger for campus donor
    await sendEmailReceipt("donor-receipts@campus-hackathon.edu", `Campaign #${event.campaignId}`, amountSol, `slot-${slot}`);
  });

  program.addEventListener("MilestoneReleasedEvent", async (event: any, slot: number) => {
    telemetry.eventsProcessed++;
    telemetry.lastSlotProcessed = slot;
    const releasedSol = event.amountReleased.toNumber() / 1e9;
    const feeSol = event.feeDeducted.toNumber() / 1e9;

    await notifyDiscord(
      "🔓 Milestone Unlocked & Payout Disbursed!",
      `Milestone stage verified and funds transferred to project creator.`,
      [
        { name: "Milestone Index", value: `#${event.milestoneIndex + 1}`, inline: true },
        { name: "Disbursed to Creator", value: `${releasedSol} SOL`, inline: true },
        { name: "Protocol Treasury Fee", value: `${feeSol} SOL`, inline: true },
        { name: "Completed?", value: event.isCompleted ? "YES 🎉" : "NO (In Progress)", inline: true },
      ]
    );
  });

  // Telemetry & Healthcheck Server
  const server = http.createServer((req, res) => {
    if (req.url === "/healthz") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          status: "UP",
          telemetry,
          timestamp: new Date().toISOString(),
        })
      );
      return;
    }
    res.writeHead(404);
    res.end();
  });

  server.listen(PORT, () => {
    console.log(`[TELEMETRY] Healthcheck probe listening at http://localhost:${PORT}/healthz`);
  });
}

runWorker().catch((err) => {
  console.error("Critical worker failure:", err);
  process.exit(1);
});